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
            default_ttl_seconds: 0,
            escalation_interval_seconds: 0,
            max_escalations: 0,
            escalation_action: EscalationAction::Unspecified as i32,
            default_icon: String::new(),
        })
        .await
        .unwrap();
}

/// Test that new notifications start in unseen state.
#[tokio::test]
async fn test_new_notification_starts_unseen() {
    let mut env = common::TestEnv::new().await;
    setup_svc(&mut env, "inbox-test").await;

    let resp = env
        .notification_client
        .send_notification(SendNotificationRequest {
            service_slug: "inbox-test".into(),
            notification_type: "msg".into(),
            recipient_user_id: "test-admin".into(),
            level: String::new(),
            payload: text_payload("Unseen test"),
            idempotency_key: String::new(),
            group_key: String::new(),
            ttl_seconds: 0,
            metadata: Default::default(),
            action_url: String::new(),
            icon: String::new(),
        })
        .await
        .unwrap()
        .into_inner();

    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

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
        NotificationState::Unseen as i32,
        "new notification should start in unseen state"
    );
}

/// Test MarkSeen transitions unseen -> unread.
#[tokio::test]
async fn test_mark_seen_transitions_to_unread() {
    let mut env = common::TestEnv::new().await;
    setup_svc(&mut env, "seen-test").await;

    let resp = env
        .notification_client
        .send_notification(SendNotificationRequest {
            service_slug: "seen-test".into(),
            notification_type: "msg".into(),
            recipient_user_id: "test-admin".into(),
            level: String::new(),
            payload: text_payload("Mark seen test"),
            idempotency_key: String::new(),
            group_key: String::new(),
            ttl_seconds: 0,
            metadata: Default::default(),
            action_url: String::new(),
            icon: String::new(),
        })
        .await
        .unwrap()
        .into_inner();

    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // Mark seen
    let seen_resp = env
        .notification_client
        .mark_seen(MarkSeenRequest {
            notification_ids: vec![resp.notification_id.clone()],
        })
        .await
        .unwrap()
        .into_inner();

    assert_eq!(seen_resp.marked_count, 1);

    let notif = env
        .notification_client
        .get_notification(GetNotificationRequest {
            notification_id: resp.notification_id,
        })
        .await
        .unwrap()
        .into_inner();

    assert_eq!(notif.state, NotificationState::Unread as i32);
    assert!(notif.seen_at.is_some());
}

/// Test GetUnseenCount returns correct count.
#[tokio::test]
async fn test_get_unseen_count() {
    let mut env = common::TestEnv::new().await;
    setup_svc(&mut env, "count-test").await;

    for _ in 0..3 {
        env.notification_client
            .send_notification(SendNotificationRequest {
                service_slug: "count-test".into(),
                notification_type: "msg".into(),
                recipient_user_id: "test-admin".into(),
                level: String::new(),
                payload: text_payload("Count me"),
                idempotency_key: String::new(),
                group_key: String::new(),
                ttl_seconds: 0,
                metadata: Default::default(),
                action_url: String::new(),
                icon: String::new(),
            })
            .await
            .unwrap();
    }

    tokio::time::sleep(std::time::Duration::from_secs(3)).await;

    let resp = env
        .notification_client
        .get_unseen_count(GetUnseenCountRequest {
            service_slugs: vec![],
        })
        .await
        .unwrap()
        .into_inner();

    assert_eq!(resp.total_count, 3);
}

/// Test ArchiveAllInGroup archives all notifications for a service.
#[tokio::test]
async fn test_archive_all_in_group() {
    let mut env = common::TestEnv::new().await;
    setup_svc(&mut env, "archive-grp").await;

    for i in 0..3 {
        env.notification_client
            .send_notification(SendNotificationRequest {
                service_slug: "archive-grp".into(),
                notification_type: "msg".into(),
                recipient_user_id: "test-admin".into(),
                level: String::new(),
                payload: text_payload(&format!("Archive {i}")),
                idempotency_key: String::new(),
                group_key: "batch".into(),
                ttl_seconds: 0,
                metadata: Default::default(),
                action_url: String::new(),
                icon: String::new(),
            })
            .await
            .unwrap();
    }

    tokio::time::sleep(std::time::Duration::from_secs(3)).await;

    let resp = env
        .notification_client
        .archive_all_in_group(ArchiveAllInGroupRequest {
            service_slug: "archive-grp".into(),
        })
        .await
        .unwrap()
        .into_inner();

    assert_eq!(resp.archived_count, 3);

    // Verify they are archived
    let list = env
        .notification_client
        .list_notifications(ListNotificationsRequest {
            states: vec![NotificationState::Archived as i32],
            service_slugs: vec!["archive-grp".into()],
            notification_types: vec![],
            group_key: String::new(),
            pagination: None,
        })
        .await
        .unwrap()
        .into_inner();

    assert_eq!(list.notifications.len(), 3);
}

/// Test that a notification with a muted service is auto-archived.
#[tokio::test]
async fn test_muted_user_auto_archives() {
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
            payload: text_payload("Should be archived"),
            idempotency_key: String::new(),
            group_key: String::new(),
            ttl_seconds: 0,
            metadata: Default::default(),
            action_url: String::new(),
            icon: String::new(),
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
        NotificationState::Archived as i32,
        "muted notification should be auto-archived"
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
            catch_up_mode: String::new(),
            sort_mode: String::new(),
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
            payload: text_payload("Should be archived"),
            idempotency_key: String::new(),
            group_key: String::new(),
            ttl_seconds: 0,
            metadata: Default::default(),
            action_url: String::new(),
            icon: String::new(),
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
        NotificationState::Archived as i32,
        "globally disabled user's notification should be auto-archived"
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
            action_url: String::new(),
            icon: String::new(),
        })
        .await;

    assert!(result.is_err());
    assert_eq!(
        result.unwrap_err().code(),
        tonic::Code::FailedPrecondition
    );
}

/// Test admin service update works end-to-end.
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
            action_url: String::new(),
            icon: String::new(),
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
            action_url: String::new(),
            icon: String::new(),
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
                action_url: String::new(),
                icon: String::new(),
            })
            .await
            .unwrap()
            .into_inner();
        ids.push(resp.notification_id);
    }

    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // Mark one as seen first, then read
    env.notification_client
        .mark_seen(MarkSeenRequest {
            notification_ids: vec![ids[0].clone()],
        })
        .await
        .unwrap();
    env.notification_client
        .mark_read(MarkReadRequest {
            notification_ids: vec![ids[0].clone()],
        })
        .await
        .unwrap();

    // List only unseen (the other one should still be unseen)
    let resp = env
        .notification_client
        .list_notifications(ListNotificationsRequest {
            states: vec![NotificationState::Unseen as i32],
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
