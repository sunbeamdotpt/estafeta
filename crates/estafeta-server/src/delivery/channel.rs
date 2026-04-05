use std::time::Duration;

use async_trait::async_trait;

use crate::db::notifications::NotificationRow;

/// Information about the recipient for delivery.
#[derive(Debug, Clone)]
pub struct RecipientInfo {
    pub user_id: String,
    pub email_address: Option<String>,
    pub phone_number: Option<String>,
    pub webhook_url: Option<String>,
    pub webhook_secret: Option<String>,
    pub push_tokens: Vec<PushTokenInfo>,
}

/// Device push token details for push notification delivery.
#[derive(Debug, Clone)]
pub struct PushTokenInfo {
    pub device_id: String,
    pub platform: String,
    pub push_token: String,
}

/// Delivery channel types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChannelType {
    Email,
    Push,
    Sms,
    Webhook,
}

impl ChannelType {
    /// Return the canonical string representation used in config and NATS subjects.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Email => "email",
            Self::Push => "push",
            Self::Sms => "sms",
            Self::Webhook => "webhook",
        }
    }

    /// Parse a channel type from its string representation.
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "email" => Some(Self::Email),
            "push" => Some(Self::Push),
            "sms" => Some(Self::Sms),
            "webhook" => Some(Self::Webhook),
            _ => None,
        }
    }
}

/// Result of a successful delivery.
#[derive(Debug)]
pub struct DeliveryResult {
    pub external_id: Option<String>,
}

/// Error from a delivery attempt.
#[derive(Debug, thiserror::Error)]
pub enum DeliveryError {
    #[error("transient error: {0}")]
    Transient(anyhow::Error),

    #[error("permanent error: {0}")]
    Permanent(anyhow::Error),

    #[error("rate limited, retry after {0:?}")]
    RateLimited(Duration),

    #[error("recipient missing required info: {0}")]
    MissingRecipientInfo(String),
}

/// Trait for delivery channel implementations.
#[async_trait]
pub trait DeliveryChannel: Send + Sync {
    fn channel_type(&self) -> ChannelType;

    async fn deliver(
        &self,
        notification: &NotificationRow,
        recipient: &RecipientInfo,
    ) -> Result<DeliveryResult, DeliveryError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_channel_type_as_str() {
        assert_eq!(ChannelType::Email.as_str(), "email");
        assert_eq!(ChannelType::Push.as_str(), "push");
        assert_eq!(ChannelType::Sms.as_str(), "sms");
        assert_eq!(ChannelType::Webhook.as_str(), "webhook");
    }

    #[test]
    fn test_channel_type_from_str() {
        assert_eq!(ChannelType::from_str("email"), Some(ChannelType::Email));
        assert_eq!(ChannelType::from_str("push"), Some(ChannelType::Push));
        assert_eq!(ChannelType::from_str("sms"), Some(ChannelType::Sms));
        assert_eq!(ChannelType::from_str("webhook"), Some(ChannelType::Webhook));
        assert_eq!(ChannelType::from_str("unknown"), None);
        assert_eq!(ChannelType::from_str(""), None);
    }

    #[test]
    fn test_channel_type_roundtrip() {
        for ct in [ChannelType::Email, ChannelType::Push, ChannelType::Sms, ChannelType::Webhook] {
            assert_eq!(ChannelType::from_str(ct.as_str()), Some(ct));
        }
    }

    #[test]
    fn test_delivery_error_display() {
        let err = DeliveryError::Transient(anyhow::anyhow!("timeout"));
        assert!(format!("{err}").contains("transient"));

        let err = DeliveryError::Permanent(anyhow::anyhow!("bad address"));
        assert!(format!("{err}").contains("permanent"));

        let err = DeliveryError::MissingRecipientInfo("email".into());
        assert!(format!("{err}").contains("email"));

        let err = DeliveryError::RateLimited(Duration::from_secs(60));
        assert!(format!("{err}").contains("60"));
    }
}
