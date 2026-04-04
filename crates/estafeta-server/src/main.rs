mod auth;
mod cache;
mod config;
mod db;
mod delivery;
mod grpc;
mod lifecycle;
mod nats;
mod processing;

use std::sync::Arc;

use sqlx::postgres::PgPoolOptions;
use tonic::transport::Server;
use tracing::info;

use estafeta_proto::estafeta::v1::{
    admin_service_server::AdminServiceServer,
    notification_service_server::NotificationServiceServer,
    schema_registry_service_server::SchemaRegistryServiceServer,
    streaming_service_server::StreamingServiceServer,
    user_config_service_server::UserConfigServiceServer,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Load config
    let config = config::Config::load()?;

    // Init tracing
    tracing_subscriber::fmt()
        .with_env_filter(&config.log_level)
        .json()
        .init();

    info!("starting estafeta notification service");

    // Database pool
    let pool = PgPoolOptions::new()
        .max_connections(config.database_max_connections)
        .connect(&config.database_url)
        .await?;
    info!("connected to PostgreSQL");

    // Run migrations
    sqlx::migrate!("../estafeta-migrations/migrations")
        .run(&pool)
        .await?;
    info!("database migrations applied");

    // NATS connection
    let nats_client = async_nats::connect(&config.nats_url).await?;
    let js = async_nats::jetstream::new(nats_client.clone());
    info!("connected to NATS");

    // Setup JetStream streams and consumers
    nats::setup_jetstream(&js).await?;

    // Publisher
    let publisher = nats::NatsPublisher::new(js.clone(), nats_client.clone());

    // Auth
    let jwks = auth::JwksClient::new(config.hydra_jwks_url.clone(), config.jwt_issuer.clone());
    jwks.refresh().await?;
    jwks.clone().spawn_refresh_task();
    let keto = auth::KetoClient::new(config.keto_url.clone());
    let interceptor = auth::AuthInterceptor::new(jwks);
    info!("auth initialized");

    // Caches
    let caches = cache::AppCaches::new();

    // Start notification processor (JetStream consumer)
    let processor_stream = js.get_stream("NOTIFICATIONS").await?;
    let processor_consumer = processor_stream
        .get_consumer::<async_nats::jetstream::consumer::pull::Config>("processor")
        .await?;

    let processor = processing::Processor::new(
        pool.clone(),
        publisher.clone(),
        caches.clone(),
        processor_consumer,
    );
    tokio::spawn(async move {
        if let Err(e) = processor.run().await {
            tracing::error!(error = %e, "notification processor crashed");
        }
    });
    info!("notification processor started");

    // Start delivery workers
    let delivery_stream = js.get_stream("DELIVERY").await?;

    // Email
    if let Some(ref smtp_config) = config.smtp {
        let email_channel = Arc::new(delivery::email::EmailChannel::new(smtp_config)?);
        let consumer = delivery_stream
            .get_consumer::<async_nats::jetstream::consumer::pull::Config>("delivery-email")
            .await?;
        let worker = delivery::DeliveryWorker::new(pool.clone(), email_channel, consumer);
        tokio::spawn(async move {
            if let Err(e) = worker.run().await {
                tracing::error!(error = %e, "email delivery worker crashed");
            }
        });
        info!("email delivery worker started");
    }

    // Push
    {
        let push_channel = Arc::new(delivery::push::PushChannel::new());
        let consumer = delivery_stream
            .get_consumer::<async_nats::jetstream::consumer::pull::Config>("delivery-push")
            .await?;
        let worker = delivery::DeliveryWorker::new(pool.clone(), push_channel, consumer);
        tokio::spawn(async move {
            if let Err(e) = worker.run().await {
                tracing::error!(error = %e, "push delivery worker crashed");
            }
        });
        info!("push delivery worker started");
    }

    // SMS
    {
        let sms_channel = Arc::new(delivery::sms::SmsChannel::new());
        let consumer = delivery_stream
            .get_consumer::<async_nats::jetstream::consumer::pull::Config>("delivery-sms")
            .await?;
        let worker = delivery::DeliveryWorker::new(pool.clone(), sms_channel, consumer);
        tokio::spawn(async move {
            if let Err(e) = worker.run().await {
                tracing::error!(error = %e, "sms delivery worker crashed");
            }
        });
        info!("sms delivery worker started");
    }

    // Webhook
    {
        let webhook_channel = Arc::new(delivery::webhook::WebhookChannel::new());
        let consumer = delivery_stream
            .get_consumer::<async_nats::jetstream::consumer::pull::Config>("delivery-webhook")
            .await?;
        let worker = delivery::DeliveryWorker::new(pool.clone(), webhook_channel, consumer);
        tokio::spawn(async move {
            if let Err(e) = worker.run().await {
                tracing::error!(error = %e, "webhook delivery worker crashed");
            }
        });
        info!("webhook delivery worker started");
    }

    // Start lifecycle scheduler
    let scheduler = lifecycle::Scheduler::new(pool.clone(), publisher.clone());
    scheduler.spawn();
    info!("lifecycle scheduler started");

    // gRPC services
    let notification_svc = grpc::notifications::NotificationServiceImpl::new(
        pool.clone(),
        keto.clone(),
        publisher.clone(),
        caches.clone(),
    );
    let schema_registry_svc =
        grpc::schema_registry::SchemaRegistryServiceImpl::new(pool.clone(), keto.clone(), caches.clone());
    let user_config_svc = grpc::user_config::UserConfigServiceImpl::new(pool.clone());
    let admin_svc = grpc::admin::AdminServiceImpl::new(pool.clone(), keto.clone(), publisher.clone());
    let streaming_svc = grpc::streaming::StreamingServiceImpl::new(nats_client);

    let addr = format!("0.0.0.0:{}", config.grpc_port).parse()?;
    info!(addr = %addr, "starting gRPC server");

    Server::builder()
        .add_service(NotificationServiceServer::with_interceptor(
            notification_svc,
            interceptor.clone(),
        ))
        .add_service(SchemaRegistryServiceServer::with_interceptor(
            schema_registry_svc,
            interceptor.clone(),
        ))
        .add_service(UserConfigServiceServer::with_interceptor(
            user_config_svc,
            interceptor.clone(),
        ))
        .add_service(AdminServiceServer::with_interceptor(
            admin_svc,
            interceptor.clone(),
        ))
        .add_service(StreamingServiceServer::with_interceptor(
            streaming_svc,
            interceptor,
        ))
        .serve(addr)
        .await?;

    Ok(())
}
