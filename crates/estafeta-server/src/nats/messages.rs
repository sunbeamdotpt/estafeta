use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Message published to notif.ingest.{service_slug} on JetStream.
#[derive(Debug, Serialize, Deserialize)]
pub struct IngestMessage {
    pub notification_id: Uuid,
    pub service_slug: String,
    pub notification_type: String,
    pub recipient_user_id: String,
    pub level: String,
    pub payload: serde_json::Value,
    pub idempotency_key: Option<String>,
    pub group_key: Option<String>,
    pub ttl_seconds: Option<i32>,
    pub metadata: HashMap<String, String>,
    pub action_url: Option<String>,
    pub icon: Option<String>,
}

/// Event published to rt.user.{user_id} on NATS Core for real-time streaming.
#[derive(Debug, Serialize, Deserialize)]
pub struct RealtimeEvent {
    pub event_type: RealtimeEventType,
    pub notification_id: Uuid,
    pub service_slug: String,
    pub notification_type: String,
    pub level: Option<String>,
    pub payload: Option<serde_json::Value>,
    pub group_key: Option<String>,
    pub metadata: Option<HashMap<String, String>>,
    pub old_state: Option<String>,
    pub new_state: Option<String>,
    pub unseen_count: Option<i64>,
    pub action_url: Option<String>,
    pub icon: Option<String>,
}

/// Discriminator for real-time event payloads.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RealtimeEventType {
    /// A brand-new notification was created.
    NewNotification,
    /// An existing notification changed state (e.g. read, archived).
    StateChange,
    /// The user's unseen count changed.
    UnseenCountUpdate,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_ingest_message_roundtrip() {
        let msg = IngestMessage {
            notification_id: Uuid::new_v4(),
            service_slug: "email-svc".into(),
            notification_type: "welcome".into(),
            recipient_user_id: "user-1".into(),
            level: "info".into(),
            payload: json!({"subject": "Welcome!", "body": "Hello"}),
            idempotency_key: Some("idem-123".into()),
            group_key: None,
            ttl_seconds: Some(3600),
            metadata: HashMap::from([("source".into(), "signup".into())]),
            action_url: Some("https://example.com/welcome".into()),
            icon: Some("mail".into()),
        };

        let bytes = serde_json::to_vec(&msg).unwrap();
        let decoded: IngestMessage = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(decoded.notification_id, msg.notification_id);
        assert_eq!(decoded.service_slug, "email-svc");
        assert_eq!(decoded.idempotency_key, Some("idem-123".into()));
        assert_eq!(decoded.action_url, Some("https://example.com/welcome".into()));
    }

    #[test]
    fn test_realtime_event_roundtrip() {
        let event = RealtimeEvent {
            event_type: RealtimeEventType::NewNotification,
            notification_id: Uuid::new_v4(),
            service_slug: "calendar".into(),
            notification_type: "reminder".into(),
            level: Some("warning".into()),
            payload: Some(json!({"title": "Meeting in 5 min"})),
            group_key: Some("meetings".into()),
            metadata: Some(HashMap::new()),
            old_state: None,
            new_state: None,
            unseen_count: None,
            action_url: None,
            icon: None,
        };

        let bytes = serde_json::to_vec(&event).unwrap();
        let decoded: RealtimeEvent = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(decoded.service_slug, "calendar");
    }

    #[test]
    fn test_realtime_event_type_serialization() {
        let json_str = serde_json::to_string(&RealtimeEventType::NewNotification).unwrap();
        assert_eq!(json_str, "\"new_notification\"");

        let json_str = serde_json::to_string(&RealtimeEventType::StateChange).unwrap();
        assert_eq!(json_str, "\"state_change\"");

        let json_str = serde_json::to_string(&RealtimeEventType::UnseenCountUpdate).unwrap();
        assert_eq!(json_str, "\"unseen_count_update\"");
    }

    #[test]
    fn test_ingest_message_optional_fields() {
        let msg = IngestMessage {
            notification_id: Uuid::new_v4(),
            service_slug: "svc".into(),
            notification_type: "type".into(),
            recipient_user_id: "u".into(),
            level: String::new(),
            payload: json!({}),
            idempotency_key: None,
            group_key: None,
            ttl_seconds: None,
            metadata: HashMap::new(),
            action_url: None,
            icon: None,
        };

        let bytes = serde_json::to_vec(&msg).unwrap();
        let decoded: IngestMessage = serde_json::from_slice(&bytes).unwrap();
        assert!(decoded.idempotency_key.is_none());
        assert!(decoded.group_key.is_none());
        assert!(decoded.ttl_seconds.is_none());
        assert!(decoded.action_url.is_none());
        assert!(decoded.icon.is_none());
    }
}
