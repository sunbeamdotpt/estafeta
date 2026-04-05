//! Database access layer (PostgreSQL via sqlx).
//!
//! Each sub-module groups queries by domain: notifications, delivery attempts,
//! user preferences, mute rules, schema registry, and service management.

pub mod delivery;
pub mod mute_rules;
pub mod notifications;
pub mod preferences;
pub mod schemas;
pub mod services;
