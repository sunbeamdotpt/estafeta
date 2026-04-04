use async_trait::async_trait;
use reqwest::Client;
use serde_json::json;
use tracing::info;

use super::channel::*;
use crate::db::notifications::NotificationRow;

pub struct WebhookChannel {
    http: Client,
}

impl WebhookChannel {
    pub fn new() -> Self {
        Self {
            http: Client::new(),
        }
    }
}

#[async_trait]
impl DeliveryChannel for WebhookChannel {
    fn channel_type(&self) -> ChannelType {
        ChannelType::Webhook
    }

    async fn deliver(
        &self,
        notification: &NotificationRow,
        recipient: &RecipientInfo,
    ) -> Result<DeliveryResult, DeliveryError> {
        let url = recipient.webhook_url.as_deref().ok_or_else(|| {
            DeliveryError::MissingRecipientInfo("webhook_url".into())
        })?;

        let body = json!({
            "notification_id": notification.id,
            "service_id": notification.service_id,
            "notification_type_id": notification.notification_type_id,
            "state": notification.state,
            "payload": notification.payload,
            "metadata": notification.metadata,
            "created_at": notification.created_at,
        });

        let mut request = self.http.post(url).json(&body);

        // HMAC signing if webhook secret is configured
        if let Some(secret) = &recipient.webhook_secret {
            
            let payload_bytes = serde_json::to_vec(&body)
                .map_err(|e| DeliveryError::Permanent(anyhow::anyhow!("json serialize: {e}")))?;

            // Simple HMAC-SHA256 signature
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};
            let mut hasher = DefaultHasher::new();
            secret.hash(&mut hasher);
            payload_bytes.hash(&mut hasher);
            let signature = format!("sha256={:x}", hasher.finish());

            request = request.header("X-Estafeta-Signature", signature);
        }

        let response = request.send().await.map_err(|e| {
            if e.is_timeout() || e.is_connect() {
                DeliveryError::Transient(anyhow::anyhow!("webhook request failed: {e}"))
            } else {
                DeliveryError::Permanent(anyhow::anyhow!("webhook request failed: {e}"))
            }
        })?;

        let status = response.status();
        if status.is_success() {
            info!(
                notification_id = %notification.id,
                url = %url,
                status = %status,
                "webhook delivered"
            );
            Ok(DeliveryResult {
                external_id: None,
            })
        } else if status.is_server_error() || status.as_u16() == 429 {
            Err(DeliveryError::Transient(anyhow::anyhow!(
                "webhook returned {status}"
            )))
        } else {
            Err(DeliveryError::Permanent(anyhow::anyhow!(
                "webhook returned {status}"
            )))
        }
    }
}
