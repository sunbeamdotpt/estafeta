use figment::{providers::Env, Figment};
use serde::Deserialize;

/// Top-level server configuration, loaded from `ESTAFETA_`-prefixed environment variables.
///
/// Nested structs (e.g. `SmtpConfig`) are populated via double-underscore separators,
/// for example `ESTAFETA_SMTP__HOST`.
#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    #[serde(default = "default_grpc_port")]
    pub grpc_port: u16,
    pub database_url: String,
    #[serde(default = "default_database_max_connections")]
    pub database_max_connections: u32,
    pub nats_url: String,
    pub hydra_jwks_url: String,
    #[serde(default)]
    pub jwt_issuer: Option<String>,
    pub keto_url: String,

    // Delivery channels (all optional)
    #[serde(default)]
    pub smtp: Option<SmtpConfig>,
    #[serde(default)]
    pub ses: Option<SesConfig>,
    #[serde(default)]
    pub fcm: Option<FcmConfig>,
    #[serde(default)]
    pub apns: Option<ApnsConfig>,
    #[serde(default)]
    pub sns: Option<SnsConfig>,

    #[serde(default = "default_log_level")]
    pub log_level: String,
}

/// SMTP configuration for email delivery.
#[derive(Debug, Deserialize, Clone)]
pub struct SmtpConfig {
    pub host: String,
    #[serde(default = "default_smtp_port")]
    pub port: u16,
    pub username: Option<String>,
    pub password: Option<String>,
    pub from_address: String,
}

/// AWS SES configuration for email delivery.
#[derive(Debug, Deserialize, Clone)]
pub struct SesConfig {
    pub region: String,
    pub from_address: String,
}

/// Firebase Cloud Messaging configuration for push notifications.
#[derive(Debug, Deserialize, Clone)]
pub struct FcmConfig {
    pub credentials_path: String,
}

/// Apple Push Notification Service configuration.
#[derive(Debug, Deserialize, Clone)]
pub struct ApnsConfig {
    pub key_path: String,
    pub key_id: String,
    pub team_id: String,
    pub topic: String,
    #[serde(default)]
    pub sandbox: bool,
}

/// AWS SNS configuration for SMS delivery.
#[derive(Debug, Deserialize, Clone)]
pub struct SnsConfig {
    pub region: String,
}

fn default_grpc_port() -> u16 {
    50051
}

fn default_database_max_connections() -> u32 {
    10
}

fn default_smtp_port() -> u16 {
    587
}

fn default_log_level() -> String {
    "info".to_string()
}

impl Config {
    /// Load configuration from `ESTAFETA_*` environment variables.
    pub fn load() -> Result<Self, figment::Error> {
        Figment::new()
            .merge(Env::prefixed("ESTAFETA_").split("__"))
            .extract()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_defaults() {
        assert_eq!(default_grpc_port(), 50051);
        assert_eq!(default_database_max_connections(), 10);
        assert_eq!(default_smtp_port(), 587);
        assert_eq!(default_log_level(), "info");
    }

    #[test]
    fn test_config_from_env() {
        // SAFETY: test-only, single-threaded test runner
        unsafe {
            std::env::set_var("ESTAFETA_DATABASE_URL", "postgres://localhost/test");
            std::env::set_var("ESTAFETA_NATS_URL", "nats://localhost:4222");
            std::env::set_var("ESTAFETA_HYDRA_JWKS_URL", "http://hydra/.well-known/jwks.json");
            std::env::set_var("ESTAFETA_KETO_URL", "http://keto:4466");
        }

        let config = Config::load().unwrap();
        assert_eq!(config.database_url, "postgres://localhost/test");
        assert_eq!(config.nats_url, "nats://localhost:4222");
        assert_eq!(config.grpc_port, 50051); // default
        assert_eq!(config.log_level, "info"); // default
        assert!(config.smtp.is_none());
        assert!(config.ses.is_none());

        // SAFETY: test-only cleanup
        unsafe {
            std::env::remove_var("ESTAFETA_DATABASE_URL");
            std::env::remove_var("ESTAFETA_NATS_URL");
            std::env::remove_var("ESTAFETA_HYDRA_JWKS_URL");
            std::env::remove_var("ESTAFETA_KETO_URL");
        }
    }
}
