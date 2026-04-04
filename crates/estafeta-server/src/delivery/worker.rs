use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use async_nats::jetstream;
use chrono::Utc;
use sqlx::PgPool;
use tracing::{error, info, warn};

use super::channel::*;
use crate::db;
use crate::nats::DeliveryDispatchMessage;

const MAX_RETRY_ATTEMPTS: i32 = 5;
const RETRY_BACKOFFS: &[u64] = &[30, 120, 600, 3600, 7200]; // seconds

/// Delivery worker: pulls from a JetStream consumer and dispatches via the appropriate channel.
pub struct DeliveryWorker {
    pool: PgPool,
    channels: HashMap<String, Arc<dyn DeliveryChannel>>,
    consumer: jetstream::consumer::Consumer<jetstream::consumer::pull::Config>,
    channel_name: String,
}

impl DeliveryWorker {
    pub fn new(
        pool: PgPool,
        channel_impl: Arc<dyn DeliveryChannel>,
        consumer: jetstream::consumer::Consumer<jetstream::consumer::pull::Config>,
    ) -> Self {
        let channel_name = channel_impl.channel_type().as_str().to_string();
        let mut channels = HashMap::new();
        channels.insert(channel_name.clone(), channel_impl);
        Self {
            pool,
            channels,
            consumer,
            channel_name,
        }
    }

    pub async fn run(self) -> Result<()> {
        info!(channel = %self.channel_name, "delivery worker started");
        let mut messages: jetstream::consumer::pull::Stream = self.consumer.messages().await?;

        use futures::StreamExt;
        while let Some(msg) = messages.next().await {
            let msg = match msg {
                Ok(m) => m,
                Err(e) => {
                    warn!(error = %e, channel = %self.channel_name, "failed to receive delivery message");
                    continue;
                }
            };

            if let Err(e) = self.process_delivery(&msg).await {
                error!(error = %e, channel = %self.channel_name, "delivery failed");
                if let Err(nak_err) = msg.ack_with(async_nats::jetstream::AckKind::Nak(None)).await {
                    error!(error = %nak_err, "failed to NAK delivery message");
                }
                continue;
            }

            if let Err(e) = msg.ack().await {
                error!(error = %e, "failed to ACK delivery message");
            }
        }

        Ok(())
    }

    async fn process_delivery(
        &self,
        msg: &async_nats::jetstream::Message,
    ) -> Result<()> {
        let dispatch: DeliveryDispatchMessage =
            serde_json::from_slice(&msg.payload)?;

        let notification = db::notifications::get_notification(&self.pool, dispatch.notification_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("notification not found: {}", dispatch.notification_id))?;

        let attempt = db::delivery::get_delivery_attempt(&self.pool, dispatch.delivery_attempt_id)
            .await?
            .ok_or_else(|| {
                anyhow::anyhow!("delivery attempt not found: {}", dispatch.delivery_attempt_id)
            })?;

        // Load recipient info
        let recipient = self.load_recipient_info(&dispatch.recipient_user_id).await?;

        let channel = self
            .channels
            .get(&dispatch.channel)
            .ok_or_else(|| anyhow::anyhow!("unknown channel: {}", dispatch.channel))?;

        match channel.deliver(&notification, &recipient).await {
            Ok(result) => {
                db::delivery::update_delivery_status(
                    &self.pool,
                    attempt.id,
                    "sent",
                    result.external_id.as_deref(),
                    None,
                )
                .await?;
                info!(
                    notification_id = %notification.id,
                    channel = %dispatch.channel,
                    "delivery successful"
                );
            }
            Err(DeliveryError::Transient(e)) => {
                let next_attempt = attempt.attempt_number + 1;
                if next_attempt > MAX_RETRY_ATTEMPTS {
                    db::delivery::update_delivery_status(
                        &self.pool,
                        attempt.id,
                        "failed",
                        None,
                        Some(&format!("max retries exceeded: {e}")),
                    )
                    .await?;
                    warn!(
                        notification_id = %notification.id,
                        channel = %dispatch.channel,
                        "delivery permanently failed after max retries"
                    );
                } else {
                    let backoff_secs =
                        RETRY_BACKOFFS.get((next_attempt - 1) as usize).copied().unwrap_or(7200);
                    let next_retry = Utc::now() + chrono::Duration::seconds(backoff_secs as i64);
                    db::delivery::schedule_retry(
                        &self.pool,
                        attempt.id,
                        next_attempt,
                        next_retry,
                        &e.to_string(),
                    )
                    .await?;
                    // NAK with delay for JetStream redelivery
                    return Err(anyhow::anyhow!("transient delivery failure: {e}"));
                }
            }
            Err(DeliveryError::Permanent(e)) => {
                db::delivery::update_delivery_status(
                    &self.pool,
                    attempt.id,
                    "failed",
                    None,
                    Some(&e.to_string()),
                )
                .await?;
                warn!(
                    notification_id = %notification.id,
                    channel = %dispatch.channel,
                    error = %e,
                    "delivery permanently failed"
                );
            }
            Err(DeliveryError::RateLimited(duration)) => {
                let next_retry = Utc::now()
                    + chrono::Duration::from_std(duration)
                        .unwrap_or(chrono::Duration::seconds(60));
                db::delivery::schedule_retry(
                    &self.pool,
                    attempt.id,
                    attempt.attempt_number,
                    next_retry,
                    "rate limited",
                )
                .await?;
                return Err(anyhow::anyhow!("rate limited"));
            }
            Err(DeliveryError::MissingRecipientInfo(field)) => {
                db::delivery::update_delivery_status(
                    &self.pool,
                    attempt.id,
                    "skipped",
                    None,
                    Some(&format!("missing recipient info: {field}")),
                )
                .await?;
            }
        }

        Ok(())
    }

    async fn load_recipient_info(&self, user_id: &str) -> Result<RecipientInfo> {
        let channel_config = db::preferences::get_channel_config(&self.pool, user_id).await?;
        let devices = db::preferences::list_devices(&self.pool, user_id).await?;

        Ok(RecipientInfo {
            user_id: user_id.to_string(),
            email_address: channel_config.as_ref().and_then(|c| c.email_address.clone()),
            phone_number: channel_config.as_ref().and_then(|c| c.phone_number.clone()),
            webhook_url: channel_config.as_ref().and_then(|c| c.webhook_url.clone()),
            webhook_secret: channel_config.as_ref().and_then(|c| c.webhook_secret.clone()),
            push_tokens: devices
                .into_iter()
                .map(|d| PushTokenInfo {
                    device_id: d.device_id,
                    platform: d.platform,
                    push_token: d.push_token,
                })
                .collect(),
        })
    }
}
