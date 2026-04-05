use anyhow::Result;
use async_nats::jetstream::{self, stream};
use tracing::info;

/// Create or update the required JetStream streams and durable consumers.
pub async fn setup_jetstream(js: &jetstream::Context) -> Result<()> {
    // NOTIFICATIONS stream — ingestion pipeline
    let notifications_stream = js
        .get_or_create_stream(stream::Config {
            name: "NOTIFICATIONS".to_string(),
            subjects: vec!["notif.ingest.>".to_string()],
            retention: stream::RetentionPolicy::WorkQueue,
            storage: stream::StorageType::File,
            ..Default::default()
        })
        .await?;
    info!(stream = "NOTIFICATIONS", "JetStream stream ready");

    // Processor consumer on NOTIFICATIONS
    notifications_stream
        .get_or_create_consumer(
            "processor",
            jetstream::consumer::pull::Config {
                durable_name: Some("processor".to_string()),
                filter_subject: "notif.ingest.>".to_string(),
                ack_wait: std::time::Duration::from_secs(30),
                max_deliver: 5,
                ..Default::default()
            },
        )
        .await?;
    info!("JetStream consumer 'processor' ready");

    Ok(())
}
