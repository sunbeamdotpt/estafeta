//! gRPC service implementations and protobuf conversion helpers.
//!
//! Each sub-module implements one gRPC service defined in the proto files:
//! admin, notifications, schema registry, real-time streaming, and user configuration.

pub mod admin;
pub mod notifications;
pub mod proto_convert;
pub mod schema_registry;
pub mod streaming;
pub mod user_config;
