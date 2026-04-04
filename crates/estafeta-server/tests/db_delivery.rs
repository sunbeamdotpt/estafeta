mod common;

use chrono::{Duration, Utc};
use estafeta_server::db::{delivery, notifications, schemas, services};
use serde_json::json;
use uuid::Uuid;

async fn setup(pool: &sqlx::PgPool) -> Uuid {
    let svc = services::insert_service(pool, "svc", "Svc", None)
        .await
        .unwrap();
    let nt = schemas::insert_notification_type(
        pool,
        svc.id,
        "type",
        "Type",
        None,
        &json!({}),
        &[],
        None,
        None,
        0,
    )
    .await
    .unwrap();

    let notif = notifications::insert_notification(
        pool,
        &notifications::InsertNotification {
            id: Uuid::new_v4(),
            service_id: svc.id,
            notification_type_id: nt.id,
            level_id: None,
            recipient_user_id: "user-1".into(),
            state: "unread".into(),
            payload: json!({}),
            group_key: None,
            idempotency_key: None,
            metadata: json!({}),
            expires_at: None,
            next_escalation_at: None,
        },
    )
    .await
    .unwrap();

    notif.id
}

#[tokio::test]
async fn test_delivery_attempt_lifecycle() {
    let db = common::TestDb::new().await;
    let notif_id = setup(&db.pool).await;

    // Insert attempt
    let attempt = delivery::insert_delivery_attempt(&db.pool, notif_id, "email")
        .await
        .unwrap();
    assert_eq!(attempt.channel, "email");
    assert_eq!(attempt.status, "pending");
    assert_eq!(attempt.attempt_number, 1);

    // Get
    let fetched = delivery::get_delivery_attempt(&db.pool, attempt.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(fetched.notification_id, notif_id);

    // Mark sent
    delivery::update_delivery_status(&db.pool, attempt.id, "sent", Some("ses-123"), None)
        .await
        .unwrap();
    let fetched = delivery::get_delivery_attempt(&db.pool, attempt.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(fetched.status, "sent");
    assert_eq!(fetched.external_id.as_deref(), Some("ses-123"));

    // List for notification
    let attempts = delivery::list_delivery_attempts_for_notification(&db.pool, notif_id)
        .await
        .unwrap();
    assert_eq!(attempts.len(), 1);
}

#[tokio::test]
async fn test_schedule_retry() {
    let db = common::TestDb::new().await;
    let notif_id = setup(&db.pool).await;

    let attempt = delivery::insert_delivery_attempt(&db.pool, notif_id, "push")
        .await
        .unwrap();

    let retry_at = Utc::now() + Duration::minutes(5);
    delivery::schedule_retry(&db.pool, attempt.id, 2, retry_at, "timeout")
        .await
        .unwrap();

    let fetched = delivery::get_delivery_attempt(&db.pool, attempt.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(fetched.attempt_number, 2);
    assert_eq!(fetched.status, "pending");
    assert_eq!(fetched.last_error.as_deref(), Some("timeout"));
    assert!(fetched.next_retry_at.is_some());
}

#[tokio::test]
async fn test_multiple_channels() {
    let db = common::TestDb::new().await;
    let notif_id = setup(&db.pool).await;

    delivery::insert_delivery_attempt(&db.pool, notif_id, "email")
        .await
        .unwrap();
    delivery::insert_delivery_attempt(&db.pool, notif_id, "push")
        .await
        .unwrap();
    delivery::insert_delivery_attempt(&db.pool, notif_id, "webhook")
        .await
        .unwrap();

    let attempts = delivery::list_delivery_attempts_for_notification(&db.pool, notif_id)
        .await
        .unwrap();
    assert_eq!(attempts.len(), 3);

    let channels: Vec<&str> = attempts.iter().map(|a| a.channel.as_str()).collect();
    assert!(channels.contains(&"email"));
    assert!(channels.contains(&"push"));
    assert!(channels.contains(&"webhook"));
}

#[tokio::test]
async fn test_mark_failed() {
    let db = common::TestDb::new().await;
    let notif_id = setup(&db.pool).await;

    let attempt = delivery::insert_delivery_attempt(&db.pool, notif_id, "sms")
        .await
        .unwrap();

    delivery::update_delivery_status(
        &db.pool,
        attempt.id,
        "failed",
        None,
        Some("invalid phone number"),
    )
    .await
    .unwrap();

    let fetched = delivery::get_delivery_attempt(&db.pool, attempt.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(fetched.status, "failed");
    assert_eq!(fetched.last_error.as_deref(), Some("invalid phone number"));
}
