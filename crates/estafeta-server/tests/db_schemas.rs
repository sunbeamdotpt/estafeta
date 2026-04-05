mod common;

use estafeta_server::db::{schemas, services};
use serde_json::json;

#[tokio::test]
async fn test_notification_type_crud() {
    let db = common::TestDb::new().await;

    let svc = services::insert_service(&db.pool, "test-svc", "Test", None)
        .await
        .unwrap();

    let schema = json!({
        "type": "object",
        "properties": {
            "title": { "type": "string" },
            "body": { "type": "string" }
        },
        "required": ["title"]
    });

    // Insert
    let nt = schemas::insert_notification_type(
        &db.pool,
        svc.id,
        "welcome",
        "Welcome Email",
        Some("Sent on signup"),
        &schema,
        Some(3600),
        Some(600),
        3,
        "resurface",
        Some("mail"),
    )
    .await
    .unwrap();

    assert_eq!(nt.type_key, "welcome");
    assert_eq!(nt.display_name, "Welcome Email");
    assert_eq!(nt.default_ttl_seconds, Some(3600));
    assert_eq!(nt.max_escalations, 3);
    assert_eq!(nt.escalation_action, "resurface");
    assert_eq!(nt.default_icon.as_deref(), Some("mail"));

    // Get
    let fetched = schemas::get_notification_type(&db.pool, svc.id, "welcome")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(fetched.id, nt.id);
    assert_eq!(fetched.json_schema, schema);

    // Update
    let updated = schemas::update_notification_type(
        &db.pool,
        svc.id,
        "welcome",
        "Welcome V2",
        None,
        &json!({"type": "object"}),
        Some(7200),
        None,
        5,
        "bump",
        Some("send"),
        true,
    )
    .await
    .unwrap();
    assert_eq!(updated.display_name, "Welcome V2");
    assert_eq!(updated.max_escalations, 5);
    assert_eq!(updated.escalation_action, "bump");
    assert_eq!(updated.default_icon.as_deref(), Some("send"));

    // List
    let list = schemas::list_notification_types(&db.pool, svc.id, 10, 0)
        .await
        .unwrap();
    assert_eq!(list.len(), 1);
}

#[tokio::test]
async fn test_notification_type_unique_constraint() {
    let db = common::TestDb::new().await;

    let svc = services::insert_service(&db.pool, "svc", "Svc", None)
        .await
        .unwrap();

    schemas::insert_notification_type(
        &db.pool,
        svc.id,
        "type1",
        "Type 1",
        None,
        &json!({}),
        None,
        None,
        0,
        "resurface",
        None,
    )
    .await
    .unwrap();

    let result = schemas::insert_notification_type(
        &db.pool,
        svc.id,
        "type1",
        "Duplicate",
        None,
        &json!({}),
        None,
        None,
        0,
        "resurface",
        None,
    )
    .await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_notification_levels() {
    let db = common::TestDb::new().await;

    let svc = services::insert_service(&db.pool, "svc", "Svc", None)
        .await
        .unwrap();

    let lvl = schemas::insert_notification_level(
        &db.pool,
        svc.id,
        "critical",
        "Critical",
        10,
        Some("#ff0000"),
        Some("alert"),
    )
    .await
    .unwrap();
    assert_eq!(lvl.key, "critical");
    assert_eq!(lvl.severity, 10);

    schemas::insert_notification_level(&db.pool, svc.id, "info", "Info", 1, None, None)
        .await
        .unwrap();

    // List should be ordered by severity DESC
    let levels = schemas::list_notification_levels(&db.pool, svc.id)
        .await
        .unwrap();
    assert_eq!(levels.len(), 2);
    assert_eq!(levels[0].key, "critical");
    assert_eq!(levels[1].key, "info");

    // Get
    let fetched = schemas::get_notification_level(&db.pool, svc.id, "critical")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(fetched.id, lvl.id);

    // Update
    let updated =
        schemas::update_notification_level(&db.pool, svc.id, "info", "Information", 2, None, None)
            .await
            .unwrap();
    assert_eq!(updated.display_name, "Information");
    assert_eq!(updated.severity, 2);
}
