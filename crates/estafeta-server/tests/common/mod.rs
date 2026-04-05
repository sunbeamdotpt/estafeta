use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;

use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use testcontainers::runners::AsyncRunner;
use testcontainers::{ContainerAsync, ImageExt};
use testcontainers_modules::{nats::Nats, postgres::Postgres};
use tonic::transport::Channel;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use estafeta_server::auth::{AuthInterceptor, JwksClient, KetoClient};
use estafeta_server::cache::AppCaches;
use estafeta_server::nats::{setup_jetstream, NatsPublisher};

use estafeta_proto::estafeta::v1::{
    admin_service_client::AdminServiceClient,
    admin_service_server::AdminServiceServer,
    notification_service_client::NotificationServiceClient,
    notification_service_server::NotificationServiceServer,
    schema_registry_service_client::SchemaRegistryServiceClient,
    schema_registry_service_server::SchemaRegistryServiceServer,
    streaming_service_client::StreamingServiceClient,
    streaming_service_server::StreamingServiceServer,
    user_config_service_client::UserConfigServiceClient,
    user_config_service_server::UserConfigServiceServer,
};

pub struct TestDb {
    pub pool: PgPool,
    _container: ContainerAsync<Postgres>,
}

impl TestDb {
    pub async fn new() -> Self {
        let container = Postgres::default()
            .start()
            .await
            .expect("failed to start postgres container — is Docker running?");

        let host = container.get_host().await.unwrap();
        let port = container.get_host_port_ipv4(5432).await.unwrap();

        let url = format!("postgres://postgres:postgres@{host}:{port}/postgres");

        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(&url)
            .await
            .unwrap();

        // Run migrations
        let migrations_path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../estafeta-migrations/migrations");
        sqlx::migrate::Migrator::new(migrations_path)
            .await
            .unwrap()
            .run(&pool)
            .await
            .unwrap();

        Self {
            pool,
            _container: container,
        }
    }
}

pub struct TestNats {
    pub client: async_nats::Client,
    pub js: async_nats::jetstream::Context,
    _container: ContainerAsync<Nats>,
}

impl TestNats {
    pub async fn new() -> Self {
        let container = Nats::default()
            .with_cmd(["-js"]) // enable JetStream
            .start()
            .await
            .expect("failed to start NATS container — is Docker running?");

        let host = container.get_host().await.unwrap();
        let port = container.get_host_port_ipv4(4222).await.unwrap();

        let url = format!("nats://{host}:{port}");

        // NATS may take a moment to be ready after container starts
        let mut client = None;
        for _ in 0..10 {
            match async_nats::connect(&url).await {
                Ok(c) => {
                    client = Some(c);
                    break;
                }
                Err(_) => {
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                }
            }
        }
        let client = client.expect("failed to connect to NATS after retries");
        let js = async_nats::jetstream::new(client.clone());

        Self {
            client,
            js,
            _container: container,
        }
    }
}

/// Full integration test environment: real PG, real NATS, wiremock for Ory, gRPC server.
pub struct TestEnv {
    pub db: TestDb,
    pub nats: TestNats,
    pub ory_mock: MockServer,
    pub admin_client: AdminServiceClient<Channel>,
    pub notification_client: NotificationServiceClient<Channel>,
    pub schema_client: SchemaRegistryServiceClient<Channel>,
    pub user_config_client: UserConfigServiceClient<Channel>,
    pub streaming_client: StreamingServiceClient<Channel>,
    pub pool: PgPool,
    pub publisher: NatsPublisher,
}

impl TestEnv {
    pub async fn new() -> Self {
        let db = TestDb::new().await;
        let nats = TestNats::new().await;

        // Setup JetStream streams and consumers
        setup_jetstream(&nats.js).await.unwrap();

        let publisher = NatsPublisher::new(nats.js.clone(), nats.client.clone());
        let caches = AppCaches::new();

        // Start wiremock for Ory endpoints
        let ory_mock = MockServer::start().await;
        setup_ory_mocks(&ory_mock).await;

        let jwks = JwksClient::new(
            format!("{}/jwks", ory_mock.uri()),
            None,
        );
        // Don't refresh — we'll use a bypass interceptor for tests
        let keto = KetoClient::new(ory_mock.uri());

        // Build gRPC services
        let notification_svc =
            estafeta_server::grpc::notifications::NotificationServiceImpl::new(
                db.pool.clone(),
                keto.clone(),
                publisher.clone(),
                caches.clone(),
            );
        let schema_svc = estafeta_server::grpc::schema_registry::SchemaRegistryServiceImpl::new(
            db.pool.clone(),
            keto.clone(),
            caches.clone(),
        );
        let user_config_svc =
            estafeta_server::grpc::user_config::UserConfigServiceImpl::new(db.pool.clone());
        let admin_svc = estafeta_server::grpc::admin::AdminServiceImpl::new(
            db.pool.clone(),
            keto.clone(),
            publisher.clone(),
        );
        let streaming_svc =
            estafeta_server::grpc::streaming::StreamingServiceImpl::new(nats.client.clone());

        // Create an interceptor that always injects test auth claims
        let test_interceptor = TestAuthInterceptor;

        // Start the processor in the background
        let processor_stream = nats.js.get_stream("NOTIFICATIONS").await.unwrap();
        let processor_consumer = processor_stream
            .get_consumer::<async_nats::jetstream::consumer::pull::Config>("processor")
            .await
            .unwrap();
        let processor = estafeta_server::processing::Processor::new(
            db.pool.clone(),
            publisher.clone(),
            caches.clone(),
            processor_consumer,
        );
        tokio::spawn(async move {
            let _ = processor.run().await;
        });

        // Start delivery workers
        let delivery_stream = nats.js.get_stream("DELIVERY").await.unwrap();
        for channel_name in &["email", "push", "sms", "webhook"] {
            let consumer_name = format!("delivery-{channel_name}");
            let consumer = delivery_stream
                .get_consumer::<async_nats::jetstream::consumer::pull::Config>(&consumer_name)
                .await
                .unwrap();

            let channel_impl: Arc<dyn estafeta_server::delivery::channel::DeliveryChannel> =
                match *channel_name {
                    "push" => Arc::new(estafeta_server::delivery::push::PushChannel::new()),
                    "sms" => Arc::new(estafeta_server::delivery::sms::SmsChannel::new()),
                    "webhook" => Arc::new(estafeta_server::delivery::webhook::WebhookChannel::new()),
                    _ => Arc::new(estafeta_server::delivery::push::PushChannel::new()), // fallback stub
                };

            let worker = estafeta_server::delivery::DeliveryWorker::new(
                db.pool.clone(),
                channel_impl,
                consumer,
            );
            tokio::spawn(async move {
                let _ = worker.run().await;
            });
        }

        // Start lifecycle scheduler
        let scheduler =
            estafeta_server::lifecycle::Scheduler::new(db.pool.clone(), publisher.clone());
        scheduler.spawn();

        // Bind gRPC server on a random port
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            let incoming = tokio_stream::wrappers::TcpListenerStream::new(listener);
            tonic::transport::Server::builder()
                .add_service(NotificationServiceServer::with_interceptor(
                    notification_svc,
                    test_interceptor.clone(),
                ))
                .add_service(SchemaRegistryServiceServer::with_interceptor(
                    schema_svc,
                    test_interceptor.clone(),
                ))
                .add_service(UserConfigServiceServer::with_interceptor(
                    user_config_svc,
                    test_interceptor.clone(),
                ))
                .add_service(AdminServiceServer::with_interceptor(
                    admin_svc,
                    test_interceptor.clone(),
                ))
                .add_service(StreamingServiceServer::with_interceptor(
                    streaming_svc,
                    test_interceptor,
                ))
                .serve_with_incoming(incoming)
                .await
                .unwrap();
        });

        // Wait for server to be ready
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        let channel = Channel::from_shared(format!("http://{addr}"))
            .unwrap()
            .connect()
            .await
            .unwrap();

        let pool = db.pool.clone();
        let pub_clone = publisher.clone();

        Self {
            db,
            nats,
            ory_mock,
            admin_client: AdminServiceClient::new(channel.clone()),
            notification_client: NotificationServiceClient::new(channel.clone()),
            schema_client: SchemaRegistryServiceClient::new(channel.clone()),
            user_config_client: UserConfigServiceClient::new(channel.clone()),
            streaming_client: StreamingServiceClient::new(channel),
            pool,
            publisher: pub_clone,
        }
    }
}

/// Test interceptor that injects auth claims for "test-admin" user with admin privileges.
#[derive(Clone)]
struct TestAuthInterceptor;

impl tonic::service::Interceptor for TestAuthInterceptor {
    fn call(
        &mut self,
        mut request: tonic::Request<()>,
    ) -> Result<tonic::Request<()>, tonic::Status> {
        request.extensions_mut().insert(estafeta_server::auth::AuthClaims {
            subject: "test-admin".to_string(),
            scopes: vec!["admin".to_string()],
        });
        Ok(request)
    }
}

/// Setup wiremock to respond to Ory Keto check requests.
/// All checks return allowed=true for test-admin.
async fn setup_ory_mocks(server: &MockServer) {
    // Keto: allow all relation-tuple checks
    Mock::given(method("POST"))
        .and(path("/relation-tuples/check"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "allowed": true
        })))
        .mount(server)
        .await;

    // JWKS endpoint (not actually used since we bypass JWT validation in tests)
    Mock::given(method("GET"))
        .and(path("/jwks"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "keys": []
        })))
        .mount(server)
        .await;
}
