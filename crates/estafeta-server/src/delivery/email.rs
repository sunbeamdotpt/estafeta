use async_trait::async_trait;
use lettre::message::header::ContentType;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};
use tracing::info;

use super::channel::*;
use crate::config::SmtpConfig;
use crate::db::notifications::NotificationRow;

pub struct EmailChannel {
    transport: AsyncSmtpTransport<Tokio1Executor>,
    from_address: String,
}

impl EmailChannel {
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

#[async_trait]
impl DeliveryChannel for EmailChannel {
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

        // Extract subject and body from payload
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

        let email = Message::builder()
            .from(
                self.from_address
                    .parse()
                    .map_err(|e| DeliveryError::Permanent(anyhow::anyhow!("bad from address: {e}")))?,
            )
            .to(to_address
                .parse()
                .map_err(|e| DeliveryError::Permanent(anyhow::anyhow!("bad to address: {e}")))?)
            .subject(subject)
            .header(ContentType::TEXT_PLAIN)
            .body(body.to_string())
            .map_err(|e| DeliveryError::Permanent(anyhow::anyhow!("failed to build email: {e}")))?;

        match self.transport.send(email).await {
            Ok(response) => {
                info!(
                    notification_id = %notification.id,
                    to = %to_address,
                    "email sent successfully"
                );
                Ok(DeliveryResult {
                    external_id: Some(response.message().collect::<Vec<&str>>().join(" ")),
                })
            }
            Err(e) => {
                // SMTP errors are generally transient
                Err(DeliveryError::Transient(anyhow::anyhow!("smtp error: {e}")))
            }
        }
    }
}
