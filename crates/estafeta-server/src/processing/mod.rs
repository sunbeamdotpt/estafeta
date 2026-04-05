//! Notification processing pipeline.
//!
//! The [`Processor`] consumes ingestion messages from JetStream, validates payloads,
//! resolves user delivery preferences, persists notifications, and fans out delivery
//! dispatch messages.

pub mod preference_resolver;
pub mod processor;
pub mod schema_validator;

pub use processor::Processor;
