//! Estafeta — unified platform notification service.
//!
//! Estafeta consolidates notifications from dozens of systems into a single
//! gRPC service backed by NATS JetStream and PostgreSQL. It supports
//! schema-validated payloads, per-user delivery preferences, multi-channel
//! dispatch, notification lifecycle management, and real-time streaming.
//!
//! # Architecture
//!
//! The service is organized into the following subsystems:
//!
//! - [`auth`] — JWT validation via Ory Hydra JWKS and authorization via Ory Keto
//! - [`cache`] — In-process moka caches for notification types, levels, and user preferences
//! - [`config`] — Environment-based configuration via figment
//! - [`db`] — PostgreSQL query layer for all persistent state
//! - [`delivery`] — Multi-channel delivery (email, push, SMS, webhook) with retry
//! - [`grpc`] — gRPC service implementations for all five API surfaces
//! - [`lifecycle`] — Notification state machine and background scheduler
//! - [`nats`] — NATS JetStream setup, publishing, and message types
//! - [`processing`] — Notification processor, preference resolver, schema validation

pub mod auth;
pub mod cache;
pub mod config;
pub mod db;
pub mod delivery;
pub mod grpc;
pub mod lifecycle;
pub mod nats;
pub mod processing;
