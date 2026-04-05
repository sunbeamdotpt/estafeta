use async_trait::async_trait;
use lettre::message::header::ContentType;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};
use tracing::info;

use super::channel::*;
use crate::config::SmtpConfig;
use crate::db::notifications::NotificationRow;

pub struct EmailChannel<T: AsyncTransport + Send + Sync = AsyncSmtpTransport<Tokio1Executor>> {
    transport: T,
    from_address: String,
}

impl EmailChannel<AsyncSmtpTransport<Tokio1Executor>> {
    pub fn new(config: &SmtpConfig) -> Result<Self, anyhow::Error> {
        let mut builder =
            AsyncSmtpTransport::<Tokio1Executor>::relay(&config.host)?
                .port(config.port);

        if let (Some(user), Some(pass)) = (&config.username, &config.password) {
            builder = builder.credentials(Credentials::new(user.clone(), pass.clone()));
        }

        Ok(Self {
            transport: builder.build(),
            from_address: config.from_address.clone(),
        })
    }
}

impl<T: AsyncTransport + Send + Sync> EmailChannel<T> {
    pub fn with_transport(transport: T, from_address: String) -> Self {
        Self {
            transport,
            from_address,
        }
    }
}

fn build_email(
    from: &str,
    to: &str,
    notification: &NotificationRow,
) -> Result<Message, DeliveryError> {
    let subject = notification
        .payload
        .get("subject")
        .and_then(|v| v.as_str())
        .unwrap_or("Notification");

    let body = notification
        .payload
        .get("body")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    Message::builder()
        .from(
            from.parse()
                .map_err(|e| DeliveryError::Permanent(anyhow::anyhow!("bad from address: {e}")))?,
        )
        .to(to
            .parse()
            .map_err(|e| DeliveryError::Permanent(anyhow::anyhow!("bad to address: {e}")))?)
        .subject(subject)
        .header(ContentType::TEXT_PLAIN)
        .body(body.to_string())
        .map_err(|e| DeliveryError::Permanent(anyhow::anyhow!("failed to build email: {e}")))
}

#[async_trait]
impl<T> DeliveryChannel for EmailChannel<T>
where
    T: AsyncTransport + Send + Sync,
    T::Error: std::fmt::Display,
{
    fn channel_type(&self) -> ChannelType {
        ChannelType::Email
    }

    async fn deliver(
        &self,
        notification: &NotificationRow,
        recipient: &RecipientInfo,
    ) -> Result<DeliveryResult, DeliveryError> {
        let to_address = recipient.email_address.as_deref().ok_or_else(|| {
            DeliveryError::MissingRecipientInfo("email_address".into())
        })?;

        let email = build_email(&self.from_address, to_address, notification)?;

        match self.transport.send(email).await {
            Ok(_response) => {
                info!(
                    notification_id = %notification.id,
                    to = %to_address,
                    "email sent successfully"
                );
                Ok(DeliveryResult {
                    external_id: None,
                })
            }
            Err(e) => {
                Err(DeliveryError::Transient(anyhow::anyhow!("smtp error: {e}")))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use serde_json::json;
    use uuid::Uuid;

    fn test_notification(payload: serde_json::Value) -> NotificationRow {
        NotificationRow {
            id: Uuid::new_v4(),
            tenant_id: None,
            service_id: Uuid::new_v4(),
            notification_type_id: Uuid::new_v4(),
            level_id: None,
            recipient_user_id: "user-1".into(),
            state: "unread".into(),
            payload,
            group_key: None,
            idempotency_key: None,
            metadata: json!({}),
            snoozed_until: None,
            expires_at: None,
            next_escalation_at: None,
            escalation_count: 0,
            read_at: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn test_recipient(email: Option<&str>) -> RecipientInfo {
        RecipientInfo {
            user_id: "user-1".into(),
            email_address: email.map(String::from),
            phone_number: None,
            webhook_url: None,
            webhook_secret: None,
            push_tokens: vec![],
        }
    }

    #[tokio::test]
    async fn test_email_delivery_with_stub() {
        let stub = lettre::transport::stub::AsyncStubTransport::new_ok();
        let channel = EmailChannel::with_transport(stub, "noreply@test.com".into());

        let notif = test_notification(json!({
            "subject": "Welcome!",
            "body": "Hello, world!"
        }));
        let recipient = test_recipient(Some("user@example.com"));

        let result = channel.deliver(&notif, &recipient).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_email_delivery_missing_recipient() {
        let stub = lettre::transport::stub::AsyncStubTransport::new_ok();
        let channel = EmailChannel::with_transport(stub, "noreply@test.com".into());

        let notif = test_notification(json!({"subject": "Test"}));
        let recipient = test_recipient(None);

        let result = channel.deliver(&notif, &recipient).await;
        assert!(matches!(result, Err(DeliveryError::MissingRecipientInfo(_))));
    }

    #[tokio::test]
    async fn test_email_delivery_defaults() {
        let stub = lettre::transport::stub::AsyncStubTransport::new_ok();
        let channel = EmailChannel::with_transport(stub, "noreply@test.com".into());

        // No subject or body in payload — should use defaults
        let notif = test_notification(json!({}));
        let recipient = test_recipient(Some("user@example.com"));

        let result = channel.deliver(&notif, &recipient).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_email_delivery_transport_error() {
        let stub = lettre::transport::stub::AsyncStubTransport::new_error();
        let channel = EmailChannel::with_transport(stub, "noreply@test.com".into());

        let notif = test_notification(json!({"subject": "Test"}));
        let recipient = test_recipient(Some("user@example.com"));

        let result = channel.deliver(&notif, &recipient).await;
        assert!(matches!(result, Err(DeliveryError::Transient(_))));
    }

    #[tokio::test]
    async fn test_email_delivery_bad_from_address() {
        let stub = lettre::transport::stub::AsyncStubTransport::new_ok();
        let channel = EmailChannel::with_transport(stub, "not-an-email".into());

        let notif = test_notification(json!({"subject": "Test"}));
        let recipient = test_recipient(Some("user@example.com"));

        let result = channel.deliver(&notif, &recipient).await;
        assert!(matches!(result, Err(DeliveryError::Permanent(_))));
    }

    #[tokio::test]
    async fn test_email_delivery_bad_to_address() {
        let stub = lettre::transport::stub::AsyncStubTransport::new_ok();
        let channel = EmailChannel::with_transport(stub, "noreply@test.com".into());

        let notif = test_notification(json!({"subject": "Test"}));
        let recipient = test_recipient(Some("not-an-email"));

        let result = channel.deliver(&notif, &recipient).await;
        assert!(matches!(result, Err(DeliveryError::Permanent(_))));
    }

    #[test]
    fn test_channel_type() {
        let stub = lettre::transport::stub::AsyncStubTransport::new_ok();
        let channel = EmailChannel::with_transport(stub, "noreply@test.com".into());
        assert_eq!(channel.channel_type(), ChannelType::Email);
    }
}
