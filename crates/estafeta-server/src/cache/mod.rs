//! In-memory caches for hot-path data (notification types, levels, user preferences).
//!
//! Backed by [`moka`] with per-cache TTLs and max capacities. These caches sit between
//! the gRPC/processing layers and the database to avoid repeated queries for data that
//! changes infrequently.

use moka::future::Cache;
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

/// Cached notification type info used during ingestion and processing.
#[derive(Debug, Clone)]
pub struct CachedNotificationType {
    pub id: Uuid,
    pub service_id: Uuid,
    pub type_key: String,
    pub json_schema: serde_json::Value,
    pub default_channels: Vec<String>,
    pub default_ttl_seconds: Option<i32>,
    pub escalation_interval_seconds: Option<i32>,
    pub max_escalations: i32,
    pub enabled: bool,
}

/// Cached notification level info.
#[derive(Debug, Clone)]
pub struct CachedNotificationLevel {
    pub id: Uuid,
    pub key: String,
    pub severity: i32,
}

/// Cached user preference resolution result.
#[derive(Debug, Clone)]
pub struct CachedUserPrefs {
    pub global_enabled: bool,
    pub service_prefs: Vec<CachedServicePref>,
    pub type_prefs: Vec<CachedTypePref>,
    pub mute_rules: Vec<CachedMuteRule>,
}

/// A user's notification preferences for a specific service.
#[derive(Debug, Clone)]
pub struct CachedServicePref {
    pub service_id: Uuid,
    pub enabled: bool,
    /// Notifications below this severity are suppressed.
    pub min_severity: Option<i32>,
    pub channels: Vec<String>,
}

/// A user's notification preferences for a specific notification type.
#[derive(Debug, Clone)]
pub struct CachedTypePref {
    pub notification_type_id: Uuid,
    pub enabled: bool,
    /// Channel overrides; `None` means fall through to service/global defaults.
    pub channels: Option<Vec<String>>,
}

/// A user-defined mute rule that suppresses matching notifications.
#[derive(Debug, Clone)]
pub struct CachedMuteRule {
    /// If `None`, applies to all services.
    pub service_id: Option<Uuid>,
    /// If `None`, applies to all notification types.
    pub notification_type_id: Option<Uuid>,
    /// If `None`, the mute is permanent (until deleted).
    pub muted_until: Option<chrono::DateTime<chrono::Utc>>,
}

/// Application-wide caches backed by moka.
#[derive(Clone)]
pub struct AppCaches {
    /// Cache key: "{service_slug}:{type_key}"
    pub notification_types: Cache<String, Arc<CachedNotificationType>>,

    /// Cache key: "{service_slug}:{level_key}"
    pub notification_levels: Cache<String, Arc<CachedNotificationLevel>>,

    /// Cache key: user_id
    pub user_prefs: Cache<String, Arc<CachedUserPrefs>>,
}

impl AppCaches {
    /// Create caches with default capacities and TTLs.
    pub fn new() -> Self {
        Self {
            notification_types: Cache::builder()
                .max_capacity(10_000)
                .time_to_live(Duration::from_secs(300))
                .build(),
            notification_levels: Cache::builder()
                .max_capacity(10_000)
                .time_to_live(Duration::from_secs(300))
                .build(),
            user_prefs: Cache::builder()
                .max_capacity(10_000)
                .time_to_live(Duration::from_secs(60))
                .build(),
        }
    }

    /// Build the composite cache key for a notification type lookup.
    pub fn notification_type_key(service_slug: &str, type_key: &str) -> String {
        format!("{service_slug}:{type_key}")
    }

    /// Build the composite cache key for a notification level lookup.
    pub fn notification_level_key(service_slug: &str, level_key: &str) -> String {
        format!("{service_slug}:{level_key}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_notification_type_key() {
        assert_eq!(
            AppCaches::notification_type_key("email-svc", "welcome"),
            "email-svc:welcome"
        );
    }

    #[test]
    fn test_notification_level_key() {
        assert_eq!(
            AppCaches::notification_level_key("email-svc", "critical"),
            "email-svc:critical"
        );
    }

    #[tokio::test]
    async fn test_cache_insert_and_get() {
        let caches = AppCaches::new();
        let key = "test-svc:test-type".to_string();
        let cached = Arc::new(CachedNotificationType {
            id: uuid::Uuid::new_v4(),
            service_id: uuid::Uuid::new_v4(),
            type_key: "test-type".into(),
            json_schema: serde_json::json!({"type": "object"}),
            default_channels: vec!["email".into()],
            default_ttl_seconds: Some(3600),
            escalation_interval_seconds: None,
            max_escalations: 0,
            enabled: true,
        });

        caches.notification_types.insert(key.clone(), cached.clone()).await;
        let result = caches.notification_types.get(&key).await;
        assert!(result.is_some());
        assert_eq!(result.unwrap().id, cached.id);
    }

    #[tokio::test]
    async fn test_cache_miss() {
        let caches = AppCaches::new();
        let result = caches.notification_types.get("nonexistent").await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_cache_invalidation() {
        let caches = AppCaches::new();
        let key = "svc:type".to_string();
        let cached = Arc::new(CachedNotificationType {
            id: uuid::Uuid::new_v4(),
            service_id: uuid::Uuid::new_v4(),
            type_key: "type".into(),
            json_schema: serde_json::json!({}),
            default_channels: vec![],
            default_ttl_seconds: None,
            escalation_interval_seconds: None,
            max_escalations: 0,
            enabled: true,
        });

        caches.notification_types.insert(key.clone(), cached).await;
        assert!(caches.notification_types.get(&key).await.is_some());

        caches.notification_types.invalidate(&key).await;
        assert!(caches.notification_types.get(&key).await.is_none());
    }
}
