//! NATS messaging layer (JetStream for durable work queues, Core for real-time events).
//!
//! Provides message types, a [`NatsPublisher`] for producing messages, and
//! [`setup_jetstream`] for bootstrapping the required streams and consumers.

pub mod messages;
pub mod publisher;
pub mod setup;

pub use messages::*;
pub use publisher::NatsPublisher;
pub use setup::setup_jetstream;
