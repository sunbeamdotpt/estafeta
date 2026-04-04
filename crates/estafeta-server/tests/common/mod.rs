use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use std::path::Path;
use testcontainers::runners::AsyncRunner;
use testcontainers::{ContainerAsync, ImageExt};
use testcontainers_modules::{nats::Nats, postgres::Postgres};

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
