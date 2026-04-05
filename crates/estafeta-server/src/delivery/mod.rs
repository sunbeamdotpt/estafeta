//! Outbound delivery subsystem.
//!
//! Defines the [`DeliveryChannel`](channel::DeliveryChannel) trait and concrete
//! implementations for email, push, SMS, and webhook delivery. The [`DeliveryWorker`]
//! pulls dispatch messages from JetStream and routes them to the appropriate channel.

pub mod channel;
pub mod email;
pub mod push;
pub mod sms;
pub mod webhook;
pub mod worker;

pub use worker::DeliveryWorker;
