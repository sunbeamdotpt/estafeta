mod common;

use estafeta_proto::estafeta::v1::*;
use std::collections::BTreeMap;

fn text_payload(title: &str) -> Option<prost_types::Struct> {
    Some(prost_types::Struct {
        fields: BTreeMap::from([(
            "title".to_string(),
            prost_types::Value {
                kind: Some(prost_types::value::Kind::StringValue(title.into())),
            },
        )]),
    })
}

fn simple_schema() -> prost_types::Struct {
    prost_types::Struct {
        fields: BTreeMap::from([(
            "type".to_string(),
            prost_types::Value {
                kind: Some(prost_types::value::Kind::StringValue("object".into())),
            },
        )]),
    }
}

async fn setup_svc(env: &mut common::TestEnv, slug: &str) {
    env.admin_client
        .register_service(RegisterServiceRequest {
            slug: slug.into(),
            display_name: slug.into(),
            description: String::new(),
        })
        .await
        .unwrap();

    env.schema_client
        .register_type(RegisterTypeRequest {
            service_slug: slug.into(),
            type_key: "msg".into(),
            display_name: "Message".into(),
            description: String::new(),
            json_schema: Some(simple_schema()),
            default_channels: vec![
                DeliveryChannel::Push as i32,
                DeliveryChannel::Sms as i32,
                DeliveryChannel::Webhook as i32,
            ],
            default_ttl_seconds: 0,
            escalation_interval_seconds: 0,
            max_escalations: 0,
        })
        .await
        .unwrap();
}

/// Test that delivery attempts are created for each configured channel.
#[tokio::test]
async fn test_delivery_attempts_created_per_channel() {
    let mut env = common::TestEnv::new().await;
    setup_svc(&mut env, "multi-ch").await;

    // Register device so push delivery has tokens
    env.user_config_client
        .register_device(RegisterDeviceRequest {
            device_id: "test-phone".into(),
            platform: "android".into(),
            push_token: "fcm-test-token".into(),
        })
        .await
        .unwrap();

    // Set up channel config for SMS/webhook
    env.user_config_client
        .update_channel_config(UpdateChannelConfigRequest {
            email_address: String::new(),
            phone_number: "+15551234567".into(),
            webhook_url: "https://webhook.test/notify".into(),
            webhook_secret: "secret123".into(),
        })
        .await
        .unwrap();

    let resp = env
        .notification_client
        .send_notification(SendNotificationRequest {
            service_slug: "multi-ch".into(),
            notification_type: "msg".into(),
            recipient_user_id: "test-admin".into(),
            level: String::new(),
            payload: text_payload("Multi-channel delivery"),
            idempotency_key: String::new(),
            group_key: String::new(),
            ttl_seconds: 0,
            metadata: Default::default(),
        })
        .await
        .unwrap()
        .into_inner();

    // Wait for processor + delivery workers
    tokio::time::sleep(std::time::Duration::from_secs(5)).await;

    // Check delivery attempts were created in DB
    let notif_id: uuid::Uuid = resp.notification_id.parse().unwrap();
    let attempts = estafeta_server::db::delivery::list_delivery_attempts_for_notification(
        &env.pool,
        notif_id,
    )
    .await
    .unwrap();

    // Should have attempts for push, sms, webhook
    assert!(
        attempts.len() >= 3,
        "expected at least 3 delivery attempts, got {}",
        attempts.len()
    );

    let channels: Vec<&str> = attempts.iter().map(|a| a.channel.as_str()).collect();
    assert!(channels.contains(&"push"), "missing push attempt");
    assert!(channels.contains(&"sms"), "missing sms attempt");
    assert!(channels.contains(&"webhook"), "missing webhook attempt");
}

/// Test that a notification with a disabled user preference is auto-dismissed.
#[tokio::test]
async fn test_muted_user_auto_dismisses() {
    let mut env = common::TestEnv::new().await;
    setup_svc(&mut env, "mute-test").await;

    // Mute the service
    env.user_config_client
        .create_mute_rule(CreateMuteRuleRequest {
            service_slug: "mute-test".into(),
            notification_type: String::new(),
            muted_until: None,
        })
        .await
        .unwrap();

    let resp = env
        .notification_client
        .send_notification(SendNotificationRequest {
            service_slug: "mute-test".into(),
            notification_type: "msg".into(),
            recipient_user_id: "test-admin".into(),
            level: String::new(),
            payload: text_payload("Should be dismissed"),
            idempotency_key: String::new(),
            group_key: String::new(),
            ttl_seconds: 0,
            metadata: Default::default(),
        })
        .await
        .unwrap()
        .into_inner();

    tokio::time::sleep(std::time::Duration::from_secs(3)).await;

    let notif = env
        .notification_client
        .get_notification(GetNotificationRequest {
            notification_id: resp.notification_id,
        })
        .await
        .unwrap()
        .into_inner();

    assert_eq!(
        notif.state,
        NotificationState::Dismissed as i32,
        "muted notification should be auto-dismissed"
    );
}

/// Test globally disabled notifications.
#[tokio::test]
async fn test_globally_disabled_user() {
    let mut env = common::TestEnv::new().await;
    setup_svc(&mut env, "global-off").await;

    // Disable notifications globally
    env.user_config_client
        .update_preferences(UpdatePreferencesRequest {
            global_enabled: false,
        })
        .await
        .unwrap();

    let resp = env
        .notification_client
        .send_notification(SendNotificationRequest {
            service_slug: "global-off".into(),
            notification_type: "msg".into(),
            recipient_user_id: "test-admin".into(),
            level: String::new(),
            payload: text_payload("Should be dismissed"),
            idempotency_key: String::new(),
            group_key: String::new(),
            ttl_seconds: 0,
            metadata: Default::default(),
        })
        .await
        .unwrap()
        .into_inner();

    tokio::time::sleep(std::time::Duration::from_secs(3)).await;

    let notif = env
        .notification_client
        .get_notification(GetNotificationRequest {
            notification_id: resp.notification_id,
        })
        .await
        .unwrap()
        .into_inner();

    assert_eq!(
        notif.state,
        NotificationState::Dismissed as i32,
        "globally disabled user's notification should be auto-dismissed"
    );
}

/// Test sending to a disabled service fails.
#[tokio::test]
async fn test_send_to_disabled_service_fails() {
    let mut env = common::TestEnv::new().await;
    setup_svc(&mut env, "dis-svc").await;

    env.admin_client
        .disable_service(DisableServiceRequest {
            slug: "dis-svc".into(),
        })
        .await
        .unwrap();

    let result = env
        .notification_client
        .send_notification(SendNotificationRequest {
            service_slug: "dis-svc".into(),
            notification_type: "msg".into(),
            recipient_user_id: "test-admin".into(),
            level: String::new(),
            payload: text_payload("Should fail"),
            idempotency_key: String::new(),
            group_key: String::new(),
            ttl_seconds: 0,
            metadata: Default::default(),
        })
        .await;

    assert!(result.is_err());
    assert_eq!(
        result.unwrap_err().code(),
        tonic::Code::FailedPrecondition
    );
}

/// Test that admin service update works end-to-end.
#[tokio::test]
async fn test_admin_update_service() {
    let mut env = common::TestEnv::new().await;

    env.admin_client
        .register_service(RegisterServiceRequest {
            slug: "upd".into(),
            display_name: "Original".into(),
            description: String::new(),
        })
        .await
        .unwrap();

    let updated = env
        .admin_client
        .update_service(UpdateServiceRequest {
            slug: "upd".into(),
            display_name: "Updated Name".into(),
            description: "Now with description".into(),
        })
        .await
        .unwrap()
        .into_inner();

    assert_eq!(updated.display_name, "Updated Name");
    assert_eq!(updated.description, "Now with description");
}

/// Test notification to nonexistent service returns NOT_FOUND.
#[tokio::test]
async fn test_send_to_nonexistent_service() {
    let mut env = common::TestEnv::new().await;

    let result = env
        .notification_client
        .send_notification(SendNotificationRequest {
            service_slug: "does-not-exist".into(),
            notification_type: "msg".into(),
            recipient_user_id: "test-admin".into(),
            level: String::new(),
            payload: text_payload("Fail"),
            idempotency_key: String::new(),
            group_key: String::new(),
            ttl_seconds: 0,
            metadata: Default::default(),
        })
        .await;

    assert!(result.is_err());
    assert_eq!(result.unwrap_err().code(), tonic::Code::NotFound);
}

/// Test notification to nonexistent type returns NOT_FOUND.
#[tokio::test]
async fn test_send_nonexistent_type() {
    let mut env = common::TestEnv::new().await;
    setup_svc(&mut env, "type-test").await;

    let result = env
        .notification_client
        .send_notification(SendNotificationRequest {
            service_slug: "type-test".into(),
            notification_type: "does-not-exist".into(),
            recipient_user_id: "test-admin".into(),
            level: String::new(),
            payload: text_payload("Fail"),
            idempotency_key: String::new(),
            group_key: String::new(),
            ttl_seconds: 0,
            metadata: Default::default(),
        })
        .await;

    assert!(result.is_err());
    assert_eq!(result.unwrap_err().code(), tonic::Code::NotFound);
}

/// Test getting a nonexistent notification.
#[tokio::test]
async fn test_get_nonexistent_notification() {
    let mut env = common::TestEnv::new().await;

    let result = env
        .notification_client
        .get_notification(GetNotificationRequest {
            notification_id: uuid::Uuid::new_v4().to_string(),
        })
        .await;

    assert!(result.is_err());
    assert_eq!(result.unwrap_err().code(), tonic::Code::NotFound);
}

/// Test list with state filter.
#[tokio::test]
async fn test_list_by_state_filter() {
    let mut env = common::TestEnv::new().await;
    setup_svc(&mut env, "state-filt").await;

    // Send 2 notifications
    let mut ids = vec![];
    for _ in 0..2 {
        let resp = env
            .notification_client
            .send_notification(SendNotificationRequest {
                service_slug: "state-filt".into(),
                notification_type: "msg".into(),
                recipient_user_id: "test-admin".into(),
                level: String::new(),
                payload: text_payload("State filter test"),
                idempotency_key: String::new(),
                group_key: String::new(),
                ttl_seconds: 0,
                metadata: Default::default(),
            })
            .await
            .unwrap()
            .into_inner();
        ids.push(resp.notification_id);
    }

    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // Mark one as read
    env.notification_client
        .mark_read(MarkReadRequest {
            notification_ids: vec![ids[0].clone()],
        })
        .await
        .unwrap();

    // List only unread
    let resp = env
        .notification_client
        .list_notifications(ListNotificationsRequest {
            states: vec![NotificationState::Unread as i32],
            service_slugs: vec![],
            notification_types: vec![],
            group_key: String::new(),
            pagination: None,
        })
        .await
        .unwrap()
        .into_inner();

    assert_eq!(resp.notifications.len(), 1);

    // List only read
    let resp = env
        .notification_client
        .list_notifications(ListNotificationsRequest {
            states: vec![NotificationState::Read as i32],
            service_slugs: vec![],
            notification_types: vec![],
            group_key: String::new(),
            pagination: None,
        })
        .await
        .unwrap()
        .into_inner();

    assert_eq!(resp.notifications.len(), 1);
}
