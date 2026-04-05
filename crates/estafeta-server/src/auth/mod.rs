//! Authentication and authorization subsystem.
//!
//! Provides JWT validation via JWKS, permission checks via Ory Keto,
//! and a gRPC interceptor that enforces authentication on every request.

pub mod claims;
pub mod interceptor;
pub mod jwks;
pub mod keto;

pub use claims::AuthClaims;
pub use interceptor::AuthInterceptor;
pub use jwks::JwksClient;
pub use keto::KetoClient;
