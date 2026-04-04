use async_trait::async_trait;
use tracing::warn;

use super::channel::*;
use crate::db::notifications::NotificationRow;

/// Push notification channel (FCM/APNs).
/// This is a stub that will be wired to real FCM/APNs clients.
pub struct PushChannel;

impl PushChannel {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl DeliveryChannel for PushChannel {
    fn channel_type(&self) -> ChannelType {
        ChannelType::Push
    }

    async fn deliver(
        &self,
        notification: &NotificationRow,
        recipient: &RecipientInfo,
    ) -> Result<DeliveryResult, DeliveryError> {
        if recipient.push_tokens.is_empty() {
            return Err(DeliveryError::MissingRecipientInfo(
                "no push tokens registered".into(),
            ));
        }

        let title = notification
            .payload
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("Notification");

        let _body = notification
            .payload
            .get("body")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        // TODO: Implement actual FCM/APNs delivery
        // For now, log and return success as a placeholder
        for token in &recipient.push_tokens {
            warn!(
                notification_id = %notification.id,
                platform = %token.platform,
                device_id = %token.device_id,
                title = %title,
                "push delivery not yet implemented — would send here"
            );
        }

        Ok(DeliveryResult {
            external_id: None,
        })
    }
}
