mod common;

use estafeta_server::nats::{
    setup_jetstream, IngestMessage, NatsPublisher, RealtimeEvent,
    RealtimeEventType,
};
use futures::StreamExt;
use std::collections::HashMap;
use uuid::Uuid;

#[tokio::test]
async fn test_jetstream_setup() {
    let nats = common::TestNats::new().await;

    setup_jetstream(&nats.js).await.unwrap();

    // Verify NOTIFICATIONS stream exists
    let mut notif_stream = nats.js.get_stream("NOTIFICATIONS").await.unwrap();
    let info = notif_stream.info().await.unwrap();
    assert_eq!(info.config.name, "NOTIFICATIONS");
}

#[tokio::test]
async fn test_jetstream_setup_idempotent() {
    let nats = common::TestNats::new().await;

    // Call setup twice — should not fail
    setup_jetstream(&nats.js).await.unwrap();
    setup_jetstream(&nats.js).await.unwrap();
}

#[tokio::test]
async fn test_publish_and_consume_ingest() {
    let nats = common::TestNats::new().await;
    setup_jetstream(&nats.js).await.unwrap();

    let publisher = NatsPublisher::new(nats.js.clone(), nats.client.clone());

    let msg = IngestMessage {
        notification_id: Uuid::new_v4(),
        service_slug: "test-svc".into(),
        notification_type: "welcome".into(),
        recipient_user_id: "user-1".into(),
        level: "info".into(),
        payload: serde_json::json!({"title": "Hello"}),
        idempotency_key: None,
        group_key: None,
        ttl_seconds: None,
        metadata: HashMap::new(),
        action_url: Some("https://example.com".into()),
        icon: Some("mail".into()),
    };

    publisher.publish_ingest("test-svc", &msg).await.unwrap();

    // Consume from processor consumer
    let stream = nats.js.get_stream("NOTIFICATIONS").await.unwrap();
    let consumer = stream
        .get_consumer::<async_nats::jetstream::consumer::pull::Config>("processor")
        .await
        .unwrap();

    let mut messages = consumer.messages().await.unwrap();
    let received = tokio::time::timeout(std::time::Duration::from_secs(5), messages.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();

    let decoded: IngestMessage = serde_json::from_slice(&received.payload).unwrap();
    assert_eq!(decoded.notification_id, msg.notification_id);
    assert_eq!(decoded.service_slug, "test-svc");
    assert_eq!(decoded.action_url, Some("https://example.com".into()));
    assert_eq!(decoded.icon, Some("mail".into()));

    received.ack().await.unwrap();
}

#[tokio::test]
async fn test_realtime_pubsub() {
    let nats = common::TestNats::new().await;

    let publisher = NatsPublisher::new(nats.js.clone(), nats.client.clone());

    // Subscribe before publishing
    let mut sub = nats
        .client
        .subscribe("rt.user.user-1")
        .await
        .unwrap();

    let event = RealtimeEvent {
        event_type: RealtimeEventType::NewNotification,
        notification_id: Uuid::new_v4(),
        service_slug: "calendar".into(),
        notification_type: "reminder".into(),
        level: Some("info".into()),
        payload: Some(serde_json::json!({"title": "Meeting"})),
        group_key: None,
        metadata: None,
        old_state: None,
        new_state: None,
        unseen_count: None,
        action_url: None,
        icon: None,
    };

    publisher.publish_realtime("user-1", &event).await.unwrap();

    let received = tokio::time::timeout(std::time::Duration::from_secs(5), sub.next())
        .await
        .unwrap()
        .unwrap();

    let decoded: RealtimeEvent = serde_json::from_slice(&received.payload).unwrap();
    assert_eq!(decoded.notification_id, event.notification_id);
    assert_eq!(decoded.service_slug, "calendar");
}

#[tokio::test]
async fn test_state_change_pubsub() {
    let nats = common::TestNats::new().await;

    let publisher = NatsPublisher::new(nats.js.clone(), nats.client.clone());

    let mut sub = nats
        .client
        .subscribe("rt.user.user-1.state")
        .await
        .unwrap();

    let event = RealtimeEvent {
        event_type: RealtimeEventType::StateChange,
        notification_id: Uuid::new_v4(),
        service_slug: String::new(),
        notification_type: String::new(),
        level: None,
        payload: None,
        group_key: None,
        metadata: None,
        old_state: Some("unseen".into()),
        new_state: Some("unread".into()),
        unseen_count: None,
        action_url: None,
        icon: None,
    };

    publisher
        .publish_state_change("user-1", &event)
        .await
        .unwrap();

    let received = tokio::time::timeout(std::time::Duration::from_secs(5), sub.next())
        .await
        .unwrap()
        .unwrap();

    let decoded: RealtimeEvent = serde_json::from_slice(&received.payload).unwrap();
    assert_eq!(decoded.old_state.as_deref(), Some("unseen"));
    assert_eq!(decoded.new_state.as_deref(), Some("unread"));
}

#[tokio::test]
async fn test_unseen_count_update_pubsub() {
    let nats = common::TestNats::new().await;

    let publisher = NatsPublisher::new(nats.js.clone(), nats.client.clone());

    let mut sub = nats
        .client
        .subscribe("rt.user.user-1")
        .await
        .unwrap();

    let event = RealtimeEvent {
        event_type: RealtimeEventType::UnseenCountUpdate,
        notification_id: Uuid::nil(),
        service_slug: String::new(),
        notification_type: String::new(),
        level: None,
        payload: None,
        group_key: None,
        metadata: None,
        old_state: None,
        new_state: None,
        unseen_count: Some(5),
        action_url: None,
        icon: None,
    };

    publisher.publish_realtime("user-1", &event).await.unwrap();

    let received = tokio::time::timeout(std::time::Duration::from_secs(5), sub.next())
        .await
        .unwrap()
        .unwrap();

    let decoded: RealtimeEvent = serde_json::from_slice(&received.payload).unwrap();
    assert_eq!(decoded.unseen_count, Some(5));
}
