mod common;

use estafeta_server::nats::{
    setup_jetstream, DeliveryDispatchMessage, IngestMessage, NatsPublisher, RealtimeEvent,
    RealtimeEventType,
};
use futures::StreamExt;
use std::collections::HashMap;
use uuid::Uuid;

#[tokio::test]
async fn test_jetstream_setup() {
    let nats = common::TestNats::new().await;

    setup_jetstream(&nats.js).await.unwrap();

    // Verify streams exist
    let mut notif_stream = nats.js.get_stream("NOTIFICATIONS").await.unwrap();
    let info = notif_stream.info().await.unwrap();
    assert_eq!(info.config.name, "NOTIFICATIONS");

    let mut delivery_stream = nats.js.get_stream("DELIVERY").await.unwrap();
    let info = delivery_stream.info().await.unwrap();
    assert_eq!(info.config.name, "DELIVERY");
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

    received.ack().await.unwrap();
}

#[tokio::test]
async fn test_publish_and_consume_delivery() {
    let nats = common::TestNats::new().await;
    setup_jetstream(&nats.js).await.unwrap();

    let publisher = NatsPublisher::new(nats.js.clone(), nats.client.clone());

    let msg = DeliveryDispatchMessage {
        notification_id: Uuid::new_v4(),
        delivery_attempt_id: Uuid::new_v4(),
        channel: "email".into(),
        recipient_user_id: "user-1".into(),
    };

    publisher.publish_delivery("email", &msg).await.unwrap();

    let stream = nats.js.get_stream("DELIVERY").await.unwrap();
    let consumer = stream
        .get_consumer::<async_nats::jetstream::consumer::pull::Config>("delivery-email")
        .await
        .unwrap();

    let mut messages = consumer.messages().await.unwrap();
    let received = tokio::time::timeout(std::time::Duration::from_secs(5), messages.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();

    let decoded: DeliveryDispatchMessage = serde_json::from_slice(&received.payload).unwrap();
    assert_eq!(decoded.notification_id, msg.notification_id);
    assert_eq!(decoded.channel, "email");

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
        unread_count: None,
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
        old_state: Some("unread".into()),
        new_state: Some("read".into()),
        unread_count: None,
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
    assert_eq!(decoded.old_state.as_deref(), Some("unread"));
    assert_eq!(decoded.new_state.as_deref(), Some("read"));
}

#[tokio::test]
async fn test_delivery_channels_independent() {
    let nats = common::TestNats::new().await;
    setup_jetstream(&nats.js).await.unwrap();

    let publisher = NatsPublisher::new(nats.js.clone(), nats.client.clone());

    // Publish to different channels
    for channel in &["email", "push", "sms", "webhook"] {
        publisher
            .publish_delivery(
                channel,
                &DeliveryDispatchMessage {
                    notification_id: Uuid::new_v4(),
                    delivery_attempt_id: Uuid::new_v4(),
                    channel: channel.to_string(),
                    recipient_user_id: "user-1".into(),
                },
            )
            .await
            .unwrap();
    }

    // Each consumer should get exactly one message
    let stream = nats.js.get_stream("DELIVERY").await.unwrap();

    for channel in &["email", "push", "sms", "webhook"] {
        let consumer_name = format!("delivery-{channel}");
        let consumer = stream
            .get_consumer::<async_nats::jetstream::consumer::pull::Config>(&consumer_name)
            .await
            .unwrap();

        let mut messages = consumer.messages().await.unwrap();
        let received = tokio::time::timeout(std::time::Duration::from_secs(5), messages.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();

        let decoded: DeliveryDispatchMessage =
            serde_json::from_slice(&received.payload).unwrap();
        assert_eq!(decoded.channel, *channel);
        received.ack().await.unwrap();
    }
}
