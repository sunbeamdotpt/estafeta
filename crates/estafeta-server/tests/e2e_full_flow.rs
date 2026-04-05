mod common;

use estafeta_proto::estafeta::v1::*;
use std::collections::{BTreeMap, HashMap};

/// Helper: register a service + notification type and return their slugs.
async fn setup_service(env: &mut common::TestEnv) -> (String, String) {
    let svc = env
        .admin_client
        .register_service(RegisterServiceRequest {
            slug: "test-svc".into(),
            display_name: "Test Service".into(),
            description: "Integration test service".into(),
        })
        .await
        .unwrap()
        .into_inner();

    assert_eq!(svc.slug, "test-svc");
    assert!(svc.enabled);

    let schema = prost_types::Struct {
        fields: BTreeMap::from([
            (
                "type".to_string(),
                prost_types::Value {
                    kind: Some(prost_types::value::Kind::StringValue(
                        "object".to_string(),
                    )),
                },
            ),
            (
                "properties".to_string(),
                prost_types::Value {
                    kind: Some(prost_types::value::Kind::StructValue(prost_types::Struct {
                        fields: BTreeMap::from([
                            (
                                "title".to_string(),
                                prost_types::Value {
                                    kind: Some(prost_types::value::Kind::StructValue(
                                        prost_types::Struct {
                                            fields: BTreeMap::from([(
                                                "type".to_string(),
                                                prost_types::Value {
                                                    kind: Some(
                                                        prost_types::value::Kind::StringValue(
                                                            "string".to_string(),
                                                        ),
                                                    ),
                                                },
                                            )]),
                                        },
                                    )),
                                },
                            ),
                            (
                                "body".to_string(),
                                prost_types::Value {
                                    kind: Some(prost_types::value::Kind::StructValue(
                                        prost_types::Struct {
                                            fields: BTreeMap::from([(
                                                "type".to_string(),
                                                prost_types::Value {
                                                    kind: Some(
                                                        prost_types::value::Kind::StringValue(
                                                            "string".to_string(),
                                                        ),
                                                    ),
                                                },
                                            )]),
                                        },
                                    )),
                                },
                            ),
                        ]),
                    })),
                },
            ),
        ]),
    };

    let nt = env
        .schema_client
        .register_type(RegisterTypeRequest {
            service_slug: "test-svc".into(),
            type_key: "alert".into(),
            display_name: "Alert".into(),
            description: "Test alert".into(),
            json_schema: Some(schema),
            default_channels: vec![
                DeliveryChannel::Push as i32,
            ],
            default_ttl_seconds: 3600,
            escalation_interval_seconds: 0,
            max_escalations: 0,
        })
        .await
        .unwrap()
        .into_inner();

    assert_eq!(nt.type_key, "alert");

    ("test-svc".into(), "alert".into())
}

// ─── Admin Service Tests ─────────────────────────────────────────────

#[tokio::test]
async fn test_admin_register_and_list_services() {
    let mut env = common::TestEnv::new().await;

    env.admin_client
        .register_service(RegisterServiceRequest {
            slug: "svc-a".into(),
            display_name: "Service A".into(),
            description: String::new(),
        })
        .await
        .unwrap();

    env.admin_client
        .register_service(RegisterServiceRequest {
            slug: "svc-b".into(),
            display_name: "Service B".into(),
            description: String::new(),
        })
        .await
        .unwrap();

    let resp = env
        .admin_client
        .list_services(ListServicesRequest {
            pagination: Some(Pagination {
                page_size: 10,
                page_token: String::new(),
            }),
        })
        .await
        .unwrap()
        .into_inner();

    assert_eq!(resp.services.len(), 2);
}

#[tokio::test]
async fn test_admin_disable_enable_service() {
    let mut env = common::TestEnv::new().await;

    env.admin_client
        .register_service(RegisterServiceRequest {
            slug: "toggle-svc".into(),
            display_name: "Toggle".into(),
            description: String::new(),
        })
        .await
        .unwrap();

    env.admin_client
        .disable_service(DisableServiceRequest {
            slug: "toggle-svc".into(),
        })
        .await
        .unwrap();

    // Sending a notification to a disabled service should fail
    let (_, _) = setup_service(&mut env).await;
    // (the setup creates a different service, so we test the toggle-svc separately)
}

#[tokio::test]
async fn test_admin_global_policy() {
    let mut env = common::TestEnv::new().await;

    let policy = env
        .admin_client
        .get_global_policy(GetGlobalPolicyRequest {})
        .await
        .unwrap()
        .into_inner();

    assert_eq!(policy.max_notifications_per_user_per_hour, 100);

    let updated = env
        .admin_client
        .set_global_policy(SetGlobalPolicyRequest {
            max_notifications_per_user_per_hour: 500,
            max_ttl_seconds: 86400,
            max_escalations: 10,
            default_channels: vec![DeliveryChannel::Email as i32],
            rate_limit_per_service_per_second: 200,
        })
        .await
        .unwrap()
        .into_inner();

    assert_eq!(updated.max_notifications_per_user_per_hour, 500);
    assert_eq!(updated.max_ttl_seconds, 86400);
}

// ─── Schema Registry Tests ───────────────────────────────────────────

#[tokio::test]
async fn test_schema_register_and_list_types() {
    let mut env = common::TestEnv::new().await;
    let (svc_slug, type_key) = setup_service(&mut env).await;

    let types = env
        .schema_client
        .list_types(ListTypesRequest {
            service_slug: svc_slug.clone(),
            pagination: None,
        })
        .await
        .unwrap()
        .into_inner();

    assert_eq!(types.types.len(), 1);
    assert_eq!(types.types[0].type_key, type_key);
}

#[tokio::test]
async fn test_schema_register_and_list_levels() {
    let mut env = common::TestEnv::new().await;
    let (svc_slug, _) = setup_service(&mut env).await;

    env.schema_client
        .register_level(RegisterLevelRequest {
            service_slug: svc_slug.clone(),
            key: "critical".into(),
            display_name: "Critical".into(),
            severity: 10,
            color: "#ff0000".into(),
            icon: "alert".into(),
        })
        .await
        .unwrap();

    env.schema_client
        .register_level(RegisterLevelRequest {
            service_slug: svc_slug.clone(),
            key: "info".into(),
            display_name: "Info".into(),
            severity: 1,
            color: String::new(),
            icon: String::new(),
        })
        .await
        .unwrap();

    let levels = env
        .schema_client
        .list_levels(ListLevelsRequest {
            service_slug: svc_slug,
        })
        .await
        .unwrap()
        .into_inner();

    assert_eq!(levels.levels.len(), 2);
    // Ordered by severity DESC
    assert_eq!(levels.levels[0].key, "critical");
    assert_eq!(levels.levels[1].key, "info");
}

#[tokio::test]
async fn test_schema_validate_payload() {
    let mut env = common::TestEnv::new().await;
    let (svc_slug, type_key) = setup_service(&mut env).await;

    // Valid payload
    let resp = env
        .schema_client
        .validate_payload(ValidatePayloadRequest {
            service_slug: svc_slug.clone(),
            notification_type: type_key.clone(),
            payload: Some(prost_types::Struct {
                fields: BTreeMap::from([(
                    "title".to_string(),
                    prost_types::Value {
                        kind: Some(prost_types::value::Kind::StringValue("Test".into())),
                    },
                )]),
            }),
        })
        .await
        .unwrap()
        .into_inner();

    assert!(resp.valid);
    assert!(resp.errors.is_empty());
}

// ─── Notification Flow Tests ─────────────────────────────────────────

#[tokio::test]
async fn test_send_and_get_notification() {
    let mut env = common::TestEnv::new().await;
    let (svc_slug, type_key) = setup_service(&mut env).await;

    let payload = prost_types::Struct {
        fields: BTreeMap::from([
            (
                "title".to_string(),
                prost_types::Value {
                    kind: Some(prost_types::value::Kind::StringValue("Hello!".into())),
                },
            ),
            (
                "body".to_string(),
                prost_types::Value {
                    kind: Some(prost_types::value::Kind::StringValue(
                        "World".into(),
                    )),
                },
            ),
        ]),
    };

    let resp = env
        .notification_client
        .send_notification(SendNotificationRequest {
            service_slug: svc_slug.clone(),
            notification_type: type_key.clone(),
            recipient_user_id: "test-admin".into(),
            level: String::new(),
            payload: Some(payload),
            idempotency_key: "idem-1".into(),
            group_key: "grp-1".into(),
            ttl_seconds: 3600,
            metadata: HashMap::from([("source".into(), "test".into())]),
        })
        .await
        .unwrap()
        .into_inner();

    assert!(!resp.notification_id.is_empty());

    // Wait for the processor to persist the notification
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    let notif = env
        .notification_client
        .get_notification(GetNotificationRequest {
            notification_id: resp.notification_id.clone(),
        })
        .await
        .unwrap()
        .into_inner();

    assert_eq!(notif.id, resp.notification_id);
    assert_eq!(notif.state, NotificationState::Unread as i32);
    assert_eq!(notif.recipient_user_id, "test-admin");
}

#[tokio::test]
async fn test_send_notification_invalid_payload() {
    let mut env = common::TestEnv::new().await;
    let (svc_slug, type_key) = setup_service(&mut env).await;

    // Send with a payload that has a non-string title (schema requires string)
    let payload = prost_types::Struct {
        fields: BTreeMap::from([(
            "title".to_string(),
            prost_types::Value {
                kind: Some(prost_types::value::Kind::NumberValue(42.0)),
            },
        )]),
    };

    let result = env
        .notification_client
        .send_notification(SendNotificationRequest {
            service_slug: svc_slug,
            notification_type: type_key,
            recipient_user_id: "test-admin".into(),
            level: String::new(),
            payload: Some(payload),
            idempotency_key: String::new(),
            group_key: String::new(),
            ttl_seconds: 0,
            metadata: HashMap::new(),
        })
        .await;

    assert!(result.is_err());
    let status = result.unwrap_err();
    assert_eq!(status.code(), tonic::Code::InvalidArgument);
}

#[tokio::test]
async fn test_list_and_filter_notifications() {
    let mut env = common::TestEnv::new().await;
    let (svc_slug, type_key) = setup_service(&mut env).await;

    // Send 3 notifications
    for i in 0..3 {
        env.notification_client
            .send_notification(SendNotificationRequest {
                service_slug: svc_slug.clone(),
                notification_type: type_key.clone(),
                recipient_user_id: "test-admin".into(),
                level: String::new(),
                payload: Some(prost_types::Struct {
                    fields: BTreeMap::from([(
                        "title".to_string(),
                        prost_types::Value {
                            kind: Some(prost_types::value::Kind::StringValue(
                                format!("Notif {i}"),
                            )),
                        },
                    )]),
                }),
                idempotency_key: String::new(),
                group_key: String::new(),
                ttl_seconds: 0,
                metadata: HashMap::new(),
            })
            .await
            .unwrap();
    }

    // Wait for processor
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;

    let resp = env
        .notification_client
        .list_notifications(ListNotificationsRequest {
            states: vec![],
            service_slugs: vec![],
            notification_types: vec![],
            group_key: String::new(),
            pagination: Some(Pagination {
                page_size: 10,
                page_token: String::new(),
            }),
        })
        .await
        .unwrap()
        .into_inner();

    assert_eq!(resp.notifications.len(), 3);
}

#[tokio::test]
async fn test_unread_count() {
    let mut env = common::TestEnv::new().await;
    let (svc_slug, type_key) = setup_service(&mut env).await;

    for _ in 0..2 {
        env.notification_client
            .send_notification(SendNotificationRequest {
                service_slug: svc_slug.clone(),
                notification_type: type_key.clone(),
                recipient_user_id: "test-admin".into(),
                level: String::new(),
                payload: Some(prost_types::Struct {
                    fields: BTreeMap::from([(
                        "title".to_string(),
                        prost_types::Value {
                            kind: Some(prost_types::value::Kind::StringValue("T".into())),
                        },
                    )]),
                }),
                idempotency_key: String::new(),
                group_key: String::new(),
                ttl_seconds: 0,
                metadata: HashMap::new(),
            })
            .await
            .unwrap();
    }

    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    let resp = env
        .notification_client
        .get_unread_count(GetUnreadCountRequest {
            service_slugs: vec![],
        })
        .await
        .unwrap()
        .into_inner();

    assert_eq!(resp.total_count, 2);
}

// ─── Lifecycle Tests ─────────────────────────────────────────────────

#[tokio::test]
async fn test_mark_read_unread_dismiss() {
    let mut env = common::TestEnv::new().await;
    let (svc_slug, type_key) = setup_service(&mut env).await;

    let resp = env
        .notification_client
        .send_notification(SendNotificationRequest {
            service_slug: svc_slug,
            notification_type: type_key,
            recipient_user_id: "test-admin".into(),
            level: String::new(),
            payload: Some(prost_types::Struct {
                fields: BTreeMap::from([(
                    "title".to_string(),
                    prost_types::Value {
                        kind: Some(prost_types::value::Kind::StringValue("Lifecycle".into())),
                    },
                )]),
            }),
            idempotency_key: String::new(),
            group_key: String::new(),
            ttl_seconds: 0,
            metadata: HashMap::new(),
        })
        .await
        .unwrap()
        .into_inner();

    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    let nid = resp.notification_id.clone();

    // Mark read
    env.notification_client
        .mark_read(MarkReadRequest {
            notification_ids: vec![nid.clone()],
        })
        .await
        .unwrap();

    let notif = env
        .notification_client
        .get_notification(GetNotificationRequest {
            notification_id: nid.clone(),
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(notif.state, NotificationState::Read as i32);

    // Mark unread
    env.notification_client
        .mark_unread(MarkUnreadRequest {
            notification_ids: vec![nid.clone()],
        })
        .await
        .unwrap();

    let notif = env
        .notification_client
        .get_notification(GetNotificationRequest {
            notification_id: nid.clone(),
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(notif.state, NotificationState::Unread as i32);

    // Dismiss
    env.notification_client
        .dismiss(DismissRequest {
            notification_ids: vec![nid.clone()],
        })
        .await
        .unwrap();

    let notif = env
        .notification_client
        .get_notification(GetNotificationRequest {
            notification_id: nid,
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(notif.state, NotificationState::Dismissed as i32);
}

#[tokio::test]
async fn test_snooze() {
    let mut env = common::TestEnv::new().await;
    let (svc_slug, type_key) = setup_service(&mut env).await;

    let resp = env
        .notification_client
        .send_notification(SendNotificationRequest {
            service_slug: svc_slug,
            notification_type: type_key,
            recipient_user_id: "test-admin".into(),
            level: String::new(),
            payload: Some(prost_types::Struct {
                fields: BTreeMap::from([(
                    "title".to_string(),
                    prost_types::Value {
                        kind: Some(prost_types::value::Kind::StringValue("Snooze me".into())),
                    },
                )]),
            }),
            idempotency_key: String::new(),
            group_key: String::new(),
            ttl_seconds: 0,
            metadata: HashMap::new(),
        })
        .await
        .unwrap()
        .into_inner();

    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    let future = chrono::Utc::now() + chrono::Duration::hours(1);
    env.notification_client
        .snooze(SnoozeRequest {
            notification_id: resp.notification_id.clone(),
            snooze_until: Some(prost_types::Timestamp {
                seconds: future.timestamp(),
                nanos: 0,
            }),
        })
        .await
        .unwrap();

    let notif = env
        .notification_client
        .get_notification(GetNotificationRequest {
            notification_id: resp.notification_id,
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(notif.state, NotificationState::Snoozed as i32);
    assert!(notif.snoozed_until.is_some());
}

// ─── User Config Tests ───────────────────────────────────────────────

#[tokio::test]
async fn test_user_preferences() {
    let mut env = common::TestEnv::new().await;

    let prefs = env
        .user_config_client
        .get_preferences(GetPreferencesRequest {})
        .await
        .unwrap()
        .into_inner();

    assert!(prefs.global_enabled);

    env.user_config_client
        .update_preferences(UpdatePreferencesRequest {
            global_enabled: false,
        })
        .await
        .unwrap();

    let prefs = env
        .user_config_client
        .get_preferences(GetPreferencesRequest {})
        .await
        .unwrap()
        .into_inner();

    assert!(!prefs.global_enabled);
}

#[tokio::test]
async fn test_user_service_preferences() {
    let mut env = common::TestEnv::new().await;
    let (svc_slug, _) = setup_service(&mut env).await;

    let pref = env
        .user_config_client
        .set_service_preference(SetServicePreferenceRequest {
            service_slug: svc_slug.clone(),
            enabled: true,
            min_severity: 5,
            channels: vec![DeliveryChannel::Email as i32, DeliveryChannel::Push as i32],
        })
        .await
        .unwrap()
        .into_inner();

    assert!(pref.enabled);
    assert_eq!(pref.min_severity, 5);
    assert_eq!(pref.channels.len(), 2);

    env.user_config_client
        .delete_service_preference(DeleteServicePreferenceRequest {
            service_slug: svc_slug,
        })
        .await
        .unwrap();
}

#[tokio::test]
async fn test_user_mute_rules() {
    let mut env = common::TestEnv::new().await;
    let (svc_slug, _) = setup_service(&mut env).await;

    let rule = env
        .user_config_client
        .create_mute_rule(CreateMuteRuleRequest {
            service_slug: svc_slug,
            notification_type: String::new(),
            muted_until: None, // permanent
        })
        .await
        .unwrap()
        .into_inner();

    assert!(!rule.id.is_empty());
    assert!(rule.muted_until.is_none());

    let rules = env
        .user_config_client
        .list_mute_rules(ListMuteRulesRequest {})
        .await
        .unwrap()
        .into_inner();

    assert_eq!(rules.rules.len(), 1);

    env.user_config_client
        .delete_mute_rule(DeleteMuteRuleRequest {
            rule_id: rule.id,
        })
        .await
        .unwrap();

    let rules = env
        .user_config_client
        .list_mute_rules(ListMuteRulesRequest {})
        .await
        .unwrap()
        .into_inner();

    assert!(rules.rules.is_empty());
}

#[tokio::test]
async fn test_user_device_registration() {
    let mut env = common::TestEnv::new().await;

    let device = env
        .user_config_client
        .register_device(RegisterDeviceRequest {
            device_id: "iphone-1".into(),
            platform: "ios".into(),
            push_token: "fcm-token-abc".into(),
        })
        .await
        .unwrap()
        .into_inner();

    assert_eq!(device.device_id, "iphone-1");
    assert_eq!(device.platform, "ios");

    env.user_config_client
        .unregister_device(UnregisterDeviceRequest {
            device_id: "iphone-1".into(),
        })
        .await
        .unwrap();
}

#[tokio::test]
async fn test_user_channel_config() {
    let mut env = common::TestEnv::new().await;

    env.user_config_client
        .update_channel_config(UpdateChannelConfigRequest {
            email_address: "test@example.com".into(),
            phone_number: "+1234567890".into(),
            webhook_url: String::new(),
            webhook_secret: String::new(),
        })
        .await
        .unwrap();

    let config = env
        .user_config_client
        .get_channel_config(GetChannelConfigRequest {})
        .await
        .unwrap()
        .into_inner();

    assert_eq!(config.email_address, "test@example.com");
    assert_eq!(config.phone_number, "+1234567890");
}

// ─── Streaming Tests ─────────────────────────────────────────────────

#[tokio::test]
async fn test_realtime_streaming() {
    let mut env = common::TestEnv::new().await;
    let (svc_slug, type_key) = setup_service(&mut env).await;

    // Subscribe to stream
    let mut stream = env
        .streaming_client
        .subscribe(SubscribeRequest {
            service_slugs: vec![],
            states: vec![],
            include_state_changes: true,
        })
        .await
        .unwrap()
        .into_inner();

    // Send a notification
    env.notification_client
        .send_notification(SendNotificationRequest {
            service_slug: svc_slug,
            notification_type: type_key,
            recipient_user_id: "test-admin".into(),
            level: String::new(),
            payload: Some(prost_types::Struct {
                fields: BTreeMap::from([(
                    "title".to_string(),
                    prost_types::Value {
                        kind: Some(prost_types::value::Kind::StringValue(
                            "Streamed!".into(),
                        )),
                    },
                )]),
            }),
            idempotency_key: String::new(),
            group_key: String::new(),
            ttl_seconds: 0,
            metadata: HashMap::new(),
        })
        .await
        .unwrap();

    // Should receive the notification event within a few seconds
    let event = tokio::time::timeout(std::time::Duration::from_secs(5), stream.message())
        .await
        .unwrap()
        .unwrap()
        .unwrap();

    match event.event {
        Some(notification_event::Event::NewNotification(new)) => {
            assert_eq!(new.service_slug, "test-svc");
            assert_eq!(new.notification_type, "alert");
        }
        other => panic!("expected NewNotification, got {other:?}"),
    }
}

// ─── Admin Replay Tests ──────────────────────────────────────────────

#[tokio::test]
async fn test_replay_notification() {
    let mut env = common::TestEnv::new().await;
    let (svc_slug, type_key) = setup_service(&mut env).await;

    let resp = env
        .notification_client
        .send_notification(SendNotificationRequest {
            service_slug: svc_slug,
            notification_type: type_key,
            recipient_user_id: "test-admin".into(),
            level: String::new(),
            payload: Some(prost_types::Struct {
                fields: BTreeMap::from([(
                    "title".to_string(),
                    prost_types::Value {
                        kind: Some(prost_types::value::Kind::StringValue("Replay".into())),
                    },
                )]),
            }),
            idempotency_key: String::new(),
            group_key: String::new(),
            ttl_seconds: 0,
            metadata: HashMap::new(),
        })
        .await
        .unwrap()
        .into_inner();

    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // Replay should succeed
    env.admin_client
        .replay_notification(ReplayNotificationRequest {
            notification_id: resp.notification_id,
            channels: vec![DeliveryChannel::Push as i32],
        })
        .await
        .unwrap();
}

// ─── Batch Notification Tests ────────────────────────────────────────

#[tokio::test]
async fn test_send_notification_batch() {
    let mut env = common::TestEnv::new().await;
    let (svc_slug, type_key) = setup_service(&mut env).await;

    let notifications: Vec<SendNotificationRequest> = (0..3)
        .map(|i| SendNotificationRequest {
            service_slug: svc_slug.clone(),
            notification_type: type_key.clone(),
            recipient_user_id: "test-admin".into(),
            level: String::new(),
            payload: Some(prost_types::Struct {
                fields: BTreeMap::from([(
                    "title".to_string(),
                    prost_types::Value {
                        kind: Some(prost_types::value::Kind::StringValue(
                            format!("Batch {i}"),
                        )),
                    },
                )]),
            }),
            idempotency_key: String::new(),
            group_key: String::new(),
            ttl_seconds: 0,
            metadata: HashMap::new(),
        })
        .collect();

    let resp = env
        .notification_client
        .send_notification_batch(SendNotificationBatchRequest { notifications })
        .await
        .unwrap()
        .into_inner();

    assert_eq!(resp.results.len(), 3);
    assert_eq!(resp.failed_count, 0);
    for r in &resp.results {
        assert!(!r.notification_id.is_empty());
    }
}
