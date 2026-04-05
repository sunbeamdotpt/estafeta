use anyhow::Result;
use async_nats::jetstream;
use tracing::warn;

use super::messages::{DeliveryDispatchMessage, IngestMessage, RealtimeEvent};

/// Publishes messages to both JetStream and NATS Core subjects.
#[derive(Clone)]
pub struct NatsPublisher {
    js: jetstream::Context,
    client: async_nats::Client,
}

impl NatsPublisher {
    /// Create a publisher with the given JetStream context and NATS client.
    pub fn new(js: jetstream::Context, client: async_nats::Client) -> Self {
        Self { js, client }
    }

    /// Publish an ingestion message to JetStream.
    pub async fn publish_ingest(&self, service_slug: &str, msg: &IngestMessage) -> Result<()> {
        let subject = format!("notif.ingest.{service_slug}");
        let payload = serde_json::to_vec(msg)?;
        self.js
            .publish(subject, payload.into())
            .await?
            .await?;
        Ok(())
    }

    /// Publish a delivery dispatch message to JetStream.
    pub async fn publish_delivery(
        &self,
        channel: &str,
        msg: &DeliveryDispatchMessage,
    ) -> Result<()> {
        let subject = format!("delivery.dispatch.{channel}");
        let payload = serde_json::to_vec(msg)?;
        self.js
            .publish(subject, payload.into())
            .await?
            .await?;
        Ok(())
    }

    /// Publish a real-time event to NATS Core (non-JetStream) for connected clients.
    pub async fn publish_realtime(&self, user_id: &str, event: &RealtimeEvent) -> Result<()> {
        let subject = format!("rt.user.{user_id}");
        let payload = serde_json::to_vec(event)?;
        if let Err(e) = self.client.publish(subject, payload.into()).await {
            // Non-critical: only affects real-time streaming, not persistence
            warn!(user_id = %user_id, error = %e, "failed to publish realtime event");
        }
        Ok(())
    }

    /// Publish a state change event to NATS Core.
    pub async fn publish_state_change(
        &self,
        user_id: &str,
        event: &RealtimeEvent,
    ) -> Result<()> {
        let subject = format!("rt.user.{user_id}.state");
        let payload = serde_json::to_vec(event)?;
        if let Err(e) = self.client.publish(subject, payload.into()).await {
            warn!(user_id = %user_id, error = %e, "failed to publish state change event");
        }
        Ok(())
    }
}
