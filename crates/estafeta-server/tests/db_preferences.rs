mod common;

use estafeta_server::db::{mute_rules, preferences, schemas, services};
use serde_json::json;

#[tokio::test]
async fn test_user_preferences_upsert() {
    let db = common::TestDb::new().await;

    // get_or_create should create with defaults
    let pref = preferences::get_or_create_user_preference(&db.pool, "user-1")
        .await
        .unwrap();
    assert!(pref.global_enabled);

    // Update
    let updated = preferences::update_user_preference(&db.pool, "user-1", false)
        .await
        .unwrap();
    assert!(!updated.global_enabled);

    // Calling again should return the same
    let again = preferences::get_or_create_user_preference(&db.pool, "user-1")
        .await
        .unwrap();
    assert!(!again.global_enabled);
}

#[tokio::test]
async fn test_service_preferences() {
    let db = common::TestDb::new().await;

    let svc = services::insert_service(&db.pool, "svc", "Svc", None)
        .await
        .unwrap();

    // Upsert
    let pref = preferences::upsert_service_preference(
        &db.pool,
        "user-1",
        svc.id,
        true,
        Some(5),
        &["email".into(), "push".into()],
    )
    .await
    .unwrap();
    assert!(pref.enabled);
    assert_eq!(pref.min_severity, Some(5));
    assert_eq!(pref.channels, vec!["email", "push"]);

    // List
    let list = preferences::list_service_preferences(&db.pool, "user-1")
        .await
        .unwrap();
    assert_eq!(list.len(), 1);

    // Update via upsert
    let updated = preferences::upsert_service_preference(
        &db.pool,
        "user-1",
        svc.id,
        false,
        None,
        &["sms".into()],
    )
    .await
    .unwrap();
    assert!(!updated.enabled);
    assert_eq!(updated.channels, vec!["sms"]);

    // Delete
    preferences::delete_service_preference(&db.pool, "user-1", svc.id)
        .await
        .unwrap();
    let list = preferences::list_service_preferences(&db.pool, "user-1")
        .await
        .unwrap();
    assert!(list.is_empty());
}

#[tokio::test]
async fn test_type_preferences() {
    let db = common::TestDb::new().await;

    let svc = services::insert_service(&db.pool, "svc", "Svc", None)
        .await
        .unwrap();
    let nt = schemas::insert_notification_type(
        &db.pool,
        svc.id,
        "alert",
        "Alert",
        None,
        &json!({}),
        &[],
        None,
        None,
        0,
    )
    .await
    .unwrap();

    let pref = preferences::upsert_type_preference(
        &db.pool,
        "user-1",
        nt.id,
        false,
        Some(&["webhook".into()]),
    )
    .await
    .unwrap();
    assert!(!pref.enabled);
    assert_eq!(pref.channels, Some(vec!["webhook".to_string()]));

    let list = preferences::list_type_preferences(&db.pool, "user-1")
        .await
        .unwrap();
    assert_eq!(list.len(), 1);

    preferences::delete_type_preference(&db.pool, "user-1", nt.id)
        .await
        .unwrap();
    let list = preferences::list_type_preferences(&db.pool, "user-1")
        .await
        .unwrap();
    assert!(list.is_empty());
}

#[tokio::test]
async fn test_channel_config() {
    let db = common::TestDb::new().await;

    // Initially none
    let config = preferences::get_channel_config(&db.pool, "user-1")
        .await
        .unwrap();
    assert!(config.is_none());

    // Upsert
    let config = preferences::upsert_channel_config(
        &db.pool,
        "user-1",
        Some("user@example.com"),
        Some("+1234567890"),
        None,
        None,
    )
    .await
    .unwrap();
    assert_eq!(config.email_address.as_deref(), Some("user@example.com"));
    assert_eq!(config.phone_number.as_deref(), Some("+1234567890"));

    // Update partial — should keep existing values via COALESCE
    let config = preferences::upsert_channel_config(
        &db.pool,
        "user-1",
        None,
        None,
        Some("https://webhook.example.com"),
        None,
    )
    .await
    .unwrap();
    assert_eq!(config.email_address.as_deref(), Some("user@example.com"));
    assert_eq!(config.webhook_url.as_deref(), Some("https://webhook.example.com"));
}

#[tokio::test]
async fn test_devices() {
    let db = common::TestDb::new().await;

    let device = preferences::upsert_device(
        &db.pool,
        "user-1",
        "device-abc",
        "ios",
        "fcm-token-123",
    )
    .await
    .unwrap();
    assert_eq!(device.device_id, "device-abc");
    assert_eq!(device.platform, "ios");

    // Update token via upsert
    let updated = preferences::upsert_device(
        &db.pool,
        "user-1",
        "device-abc",
        "ios",
        "new-token-456",
    )
    .await
    .unwrap();
    assert_eq!(updated.push_token, "new-token-456");

    let devices = preferences::list_devices(&db.pool, "user-1")
        .await
        .unwrap();
    assert_eq!(devices.len(), 1);

    preferences::delete_device(&db.pool, "user-1", "device-abc")
        .await
        .unwrap();
    let devices = preferences::list_devices(&db.pool, "user-1")
        .await
        .unwrap();
    assert!(devices.is_empty());
}

#[tokio::test]
async fn test_global_policy() {
    let db = common::TestDb::new().await;

    let policy = preferences::get_global_policy(&db.pool).await.unwrap();
    assert_eq!(policy.max_notifications_per_user_per_hour, 100);
    assert_eq!(policy.max_ttl_seconds, 2592000);
    assert_eq!(policy.max_escalations, 5);

    let updated = preferences::update_global_policy(
        &db.pool,
        200,
        86400,
        10,
        &["email".into(), "push".into()],
        50,
    )
    .await
    .unwrap();
    assert_eq!(updated.max_notifications_per_user_per_hour, 200);
    assert_eq!(updated.max_ttl_seconds, 86400);
    assert_eq!(updated.default_channels, vec!["email", "push"]);
}

#[tokio::test]
async fn test_mute_rules() {
    let db = common::TestDb::new().await;

    let svc = services::insert_service(&db.pool, "svc", "Svc", None)
        .await
        .unwrap();

    // Create permanent mute for a service
    let rule = mute_rules::insert_mute_rule(&db.pool, "user-1", Some(svc.id), None, None)
        .await
        .unwrap();
    assert!(rule.muted_until.is_none());

    // Create temporary mute
    let future = chrono::Utc::now() + chrono::Duration::hours(1);
    mute_rules::insert_mute_rule(&db.pool, "user-1", None, None, Some(future))
        .await
        .unwrap();

    let all = mute_rules::list_mute_rules(&db.pool, "user-1")
        .await
        .unwrap();
    assert_eq!(all.len(), 2);

    let active = mute_rules::list_active_mute_rules(&db.pool, "user-1")
        .await
        .unwrap();
    assert_eq!(active.len(), 2);

    // Delete one
    let deleted = mute_rules::delete_mute_rule(&db.pool, "user-1", rule.id)
        .await
        .unwrap();
    assert!(deleted);

    let all = mute_rules::list_mute_rules(&db.pool, "user-1")
        .await
        .unwrap();
    assert_eq!(all.len(), 1);

    // Can't delete other user's rule
    let deleted = mute_rules::delete_mute_rule(&db.pool, "user-2", all[0].id)
        .await
        .unwrap();
    assert!(!deleted);
}
