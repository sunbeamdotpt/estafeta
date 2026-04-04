use async_trait::async_trait;
use tracing::warn;

use super::channel::*;
use crate::db::notifications::NotificationRow;

/// SMS delivery channel.
/// Stub implementation — wire to AWS SNS, Twilio, etc.
pub struct SmsChannel;

impl SmsChannel {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl DeliveryChannel for SmsChannel {
    fn channel_type(&self) -> ChannelType {
        ChannelType::Sms
    }

    async fn deliver(
        &self,
        notification: &NotificationRow,
        recipient: &RecipientInfo,
    ) -> Result<DeliveryResult, DeliveryError> {
        let phone = recipient.phone_number.as_deref().ok_or_else(|| {
            DeliveryError::MissingRecipientInfo("phone_number".into())
        })?;

        let body = notification
            .payload
            .get("sms_body")
            .or_else(|| notification.payload.get("body"))
            .and_then(|v| v.as_str())
            .unwrap_or("You have a new notification");

        // TODO: Implement actual SMS delivery via SNS/Twilio
        warn!(
            notification_id = %notification.id,
            phone = %phone,
            body = %body,
            "SMS delivery not yet implemented — would send here"
        );

        Ok(DeliveryResult {
            external_id: None,
        })
    }
}
