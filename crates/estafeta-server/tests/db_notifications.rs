mod common;

use chrono::{Duration, Utc};
use estafeta_server::db::{notifications, schemas, services};
use serde_json::json;
use uuid::Uuid;

async fn setup_service_and_type(pool: &sqlx::PgPool) -> (Uuid, Uuid) {
    let svc = services::insert_service(pool, "test-svc", "Test", None)
        .await
        .unwrap();
    let nt = schemas::insert_notification_type(
        pool,
        svc.id,
        "alert",
        "Alert",
        None,
        &json!({"type": "object"}),
        &[],
        None,
        None,
        0,
    )
    .await
    .unwrap();
    (svc.id, nt.id)
}

#[tokio::test]
async fn test_insert_and_get_notification() {
    let db = common::TestDb::new().await;
    let (svc_id, type_id) = setup_service_and_type(&db.pool).await;

    let notif = notifications::insert_notification(
        &db.pool,
        &notifications::InsertNotification {
            id: Uuid::new_v4(),
            service_id: svc_id,
            notification_type_id: type_id,
            level_id: None,
            recipient_user_id: "user-1".into(),
            state: "unread".into(),
            payload: json!({"title": "Hello"}),
            group_key: Some("grp-1".into()),
            idempotency_key: Some("idem-1".into()),
            metadata: json!({"source": "test"}),
            expires_at: None,
            next_escalation_at: None,
        },
    )
    .await
    .unwrap();

    assert_eq!(notif.state, "unread");
    assert_eq!(notif.recipient_user_id, "user-1");

    let fetched = notifications::get_notification(&db.pool, notif.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(fetched.id, notif.id);
    assert_eq!(fetched.payload, json!({"title": "Hello"}));
}

#[tokio::test]
async fn test_idempotency_key_dedup() {
    let db = common::TestDb::new().await;
    let (svc_id, type_id) = setup_service_and_type(&db.pool).await;

    let insert = notifications::InsertNotification {
        id: Uuid::new_v4(),
        service_id: svc_id,
        notification_type_id: type_id,
        level_id: None,
        recipient_user_id: "user-1".into(),
        state: "unread".into(),
        payload: json!({}),
        group_key: None,
        idempotency_key: Some("unique-key".into()),
        metadata: json!({}),
        expires_at: None,
        next_escalation_at: None,
    };

    notifications::insert_notification(&db.pool, &insert)
        .await
        .unwrap();

    let mut dup = insert;
    dup.id = Uuid::new_v4();
    let result = notifications::insert_notification(&db.pool, &dup).await;
    assert!(result.is_err()); // unique constraint violation
}

#[tokio::test]
async fn test_list_notifications_with_filters() {
    let db = common::TestDb::new().await;
    let (svc_id, type_id) = setup_service_and_type(&db.pool).await;

    for i in 0..5 {
        notifications::insert_notification(
            &db.pool,
            &notifications::InsertNotification {
                id: Uuid::new_v4(),
                service_id: svc_id,
                notification_type_id: type_id,
                level_id: None,
                recipient_user_id: "user-1".into(),
                state: if i < 3 { "unread" } else { "read" }.into(),
                payload: json!({}),
                group_key: if i == 0 { Some("grp".into()) } else { None },
                idempotency_key: None,
                metadata: json!({}),
                expires_at: None,
                next_escalation_at: None,
            },
        )
        .await
        .unwrap();
    }

    // All notifications for user
    let all = notifications::list_notifications(&db.pool, "user-1", &[], &[], &[], None, 10, 0)
        .await
        .unwrap();
    assert_eq!(all.len(), 5);

    // Filter by state
    let unread = notifications::list_notifications(
        &db.pool,
        "user-1",
        &["unread".into()],
        &[],
        &[],
        None,
        10,
        0,
    )
    .await
    .unwrap();
    assert_eq!(unread.len(), 3);

    // Filter by group_key
    let grouped = notifications::list_notifications(
        &db.pool,
        "user-1",
        &[],
        &[],
        &[],
        Some("grp"),
        10,
        0,
    )
    .await
    .unwrap();
    assert_eq!(grouped.len(), 1);

    // Pagination
    let page = notifications::list_notifications(&db.pool, "user-1", &[], &[], &[], None, 2, 0)
        .await
        .unwrap();
    assert_eq!(page.len(), 2);
}

#[tokio::test]
async fn test_mark_read_unread() {
    let db = common::TestDb::new().await;
    let (svc_id, type_id) = setup_service_and_type(&db.pool).await;

    let notif = notifications::insert_notification(
        &db.pool,
        &notifications::InsertNotification {
            id: Uuid::new_v4(),
            service_id: svc_id,
            notification_type_id: type_id,
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

    // Mark read
    let affected = notifications::mark_read(&db.pool, "user-1", &[notif.id])
        .await
        .unwrap();
    assert_eq!(affected, 1);

    let fetched = notifications::get_notification(&db.pool, notif.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(fetched.state, "read");
    assert!(fetched.read_at.is_some());

    // Mark unread
    let affected = notifications::mark_unread(&db.pool, "user-1", &[notif.id])
        .await
        .unwrap();
    assert_eq!(affected, 1);

    let fetched = notifications::get_notification(&db.pool, notif.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(fetched.state, "unread");
    assert!(fetched.read_at.is_none());
}

#[tokio::test]
async fn test_snooze_and_wake() {
    let db = common::TestDb::new().await;
    let (svc_id, type_id) = setup_service_and_type(&db.pool).await;

    let notif = notifications::insert_notification(
        &db.pool,
        &notifications::InsertNotification {
            id: Uuid::new_v4(),
            service_id: svc_id,
            notification_type_id: type_id,
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

    // Snooze until the past (so wake_snoozed picks it up immediately)
    let past = Utc::now() - Duration::seconds(10);
    notifications::snooze(&db.pool, "user-1", notif.id, past)
        .await
        .unwrap();

    let fetched = notifications::get_notification(&db.pool, notif.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(fetched.state, "snoozed");

    // Wake
    let woken = notifications::wake_snoozed(&db.pool, 100).await.unwrap();
    assert_eq!(woken.len(), 1);
    assert_eq!(woken[0].id, notif.id);

    let fetched = notifications::get_notification(&db.pool, notif.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(fetched.state, "unread");
}

#[tokio::test]
async fn test_dismiss() {
    let db = common::TestDb::new().await;
    let (svc_id, type_id) = setup_service_and_type(&db.pool).await;

    let notif = notifications::insert_notification(
        &db.pool,
        &notifications::InsertNotification {
            id: Uuid::new_v4(),
            service_id: svc_id,
            notification_type_id: type_id,
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

    let affected = notifications::dismiss(&db.pool, "user-1", &[notif.id])
        .await
        .unwrap();
    assert_eq!(affected, 1);

    let fetched = notifications::get_notification(&db.pool, notif.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(fetched.state, "dismissed");

    // Can't dismiss again
    let affected = notifications::dismiss(&db.pool, "user-1", &[notif.id])
        .await
        .unwrap();
    assert_eq!(affected, 0);
}

#[tokio::test]
async fn test_expire_notifications() {
    let db = common::TestDb::new().await;
    let (svc_id, type_id) = setup_service_and_type(&db.pool).await;

    let past = Utc::now() - Duration::seconds(10);

    notifications::insert_notification(
        &db.pool,
        &notifications::InsertNotification {
            id: Uuid::new_v4(),
            service_id: svc_id,
            notification_type_id: type_id,
            level_id: None,
            recipient_user_id: "user-1".into(),
            state: "unread".into(),
            payload: json!({}),
            group_key: None,
            idempotency_key: None,
            metadata: json!({}),
            expires_at: Some(past),
            next_escalation_at: None,
        },
    )
    .await
    .unwrap();

    let expired = notifications::expire_notifications(&db.pool, 100)
        .await
        .unwrap();
    assert_eq!(expired.len(), 1);
    assert_eq!(expired[0].state, "expired");
}

#[tokio::test]
async fn test_unread_count() {
    let db = common::TestDb::new().await;
    let (svc_id, type_id) = setup_service_and_type(&db.pool).await;

    for _ in 0..3 {
        notifications::insert_notification(
            &db.pool,
            &notifications::InsertNotification {
                id: Uuid::new_v4(),
                service_id: svc_id,
                notification_type_id: type_id,
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
    }

    let count = notifications::count_unread(&db.pool, "user-1", &[])
        .await
        .unwrap();
    assert_eq!(count, 3);

    let by_svc = notifications::count_unread_by_service(&db.pool, "user-1")
        .await
        .unwrap();
    assert_eq!(by_svc.len(), 1);
    assert_eq!(by_svc[0].1, 3);

    // Different user has 0
    let count = notifications::count_unread(&db.pool, "user-2", &[])
        .await
        .unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn test_escalation_due() {
    let db = common::TestDb::new().await;
    let (svc_id, type_id) = setup_service_and_type(&db.pool).await;

    let past = Utc::now() - Duration::seconds(10);

    let notif = notifications::insert_notification(
        &db.pool,
        &notifications::InsertNotification {
            id: Uuid::new_v4(),
            service_id: svc_id,
            notification_type_id: type_id,
            level_id: None,
            recipient_user_id: "user-1".into(),
            state: "unread".into(),
            payload: json!({}),
            group_key: None,
            idempotency_key: None,
            metadata: json!({}),
            expires_at: None,
            next_escalation_at: Some(past),
        },
    )
    .await
    .unwrap();

    let due = notifications::get_escalation_due(&db.pool, 100)
        .await
        .unwrap();
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].id, notif.id);

    // Update escalation
    let future = Utc::now() + Duration::hours(1);
    notifications::update_escalation(&db.pool, notif.id, 1, Some(future))
        .await
        .unwrap();

    // Should not be due now
    let due = notifications::get_escalation_due(&db.pool, 100)
        .await
        .unwrap();
    assert_eq!(due.len(), 0);
}
