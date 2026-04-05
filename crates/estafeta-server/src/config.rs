use figment::{providers::Env, Figment};
use serde::Deserialize;

/// Top-level server configuration, loaded from `ESTAFETA_`-prefixed environment variables.
///
/// Nested structs are populated via double-underscore separators,
/// for example `ESTAFETA_NATS_URL`.
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

    #[serde(default = "default_log_level")]
    pub log_level: String,
}

fn default_grpc_port() -> u16 {
    50051
}

fn default_database_max_connections() -> u32 {
    10
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

        // SAFETY: test-only cleanup
        unsafe {
            std::env::remove_var("ESTAFETA_DATABASE_URL");
            std::env::remove_var("ESTAFETA_NATS_URL");
            std::env::remove_var("ESTAFETA_HYDRA_JWKS_URL");
            std::env::remove_var("ESTAFETA_KETO_URL");
        }
    }
}
