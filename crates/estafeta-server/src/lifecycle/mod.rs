//! Notification lifecycle management.
//!
//! Contains the state machine governing notification states (unseen, unread, read, snoozed,
//! archived, expired) and a background [`Scheduler`] that handles snooze wake-ups,
//! TTL expiry, and escalation.

pub mod scheduler;
pub mod state_machine;

pub use scheduler::Scheduler;
