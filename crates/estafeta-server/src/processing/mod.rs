//! Notification processing pipeline.
//!
//! The [`Processor`] consumes ingestion messages from JetStream, validates payloads,
//! resolves user preferences, persists notifications, and publishes real-time events.

pub mod preference_resolver;
pub mod processor;
pub mod schema_validator;

pub use processor::Processor;
