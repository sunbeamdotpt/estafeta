mod common;

use estafeta_server::db::services;

#[tokio::test]
async fn test_insert_and_get_service() {
    let db = common::TestDb::new().await;

    let row = services::insert_service(&db.pool, "email-svc", "Email Service", Some("Sends emails"))
        .await
        .unwrap();

    assert_eq!(row.slug, "email-svc");
    assert_eq!(row.display_name, "Email Service");
    assert_eq!(row.description.as_deref(), Some("Sends emails"));
    assert!(row.enabled);

    let fetched = services::get_service_by_slug(&db.pool, "email-svc")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(fetched.id, row.id);
}

#[tokio::test]
async fn test_get_nonexistent_service() {
    let db = common::TestDb::new().await;

    let result = services::get_service_by_slug(&db.pool, "nonexistent")
        .await
        .unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn test_list_services() {
    let db = common::TestDb::new().await;

    services::insert_service(&db.pool, "svc-a", "Service A", None)
        .await
        .unwrap();
    services::insert_service(&db.pool, "svc-b", "Service B", None)
        .await
        .unwrap();

    let all = services::list_services(&db.pool, 10, 0).await.unwrap();
    assert_eq!(all.len(), 2);

    let page = services::list_services(&db.pool, 1, 0).await.unwrap();
    assert_eq!(page.len(), 1);
}

#[tokio::test]
async fn test_update_service() {
    let db = common::TestDb::new().await;

    services::insert_service(&db.pool, "svc", "Original", None)
        .await
        .unwrap();

    let updated = services::update_service(&db.pool, "svc", "Updated Name", Some("New desc"))
        .await
        .unwrap();

    assert_eq!(updated.display_name, "Updated Name");
    assert_eq!(updated.description.as_deref(), Some("New desc"));
}

#[tokio::test]
async fn test_disable_enable_service() {
    let db = common::TestDb::new().await;

    services::insert_service(&db.pool, "svc", "Svc", None)
        .await
        .unwrap();

    services::set_service_enabled(&db.pool, "svc", false)
        .await
        .unwrap();
    let svc = services::get_service_by_slug(&db.pool, "svc")
        .await
        .unwrap()
        .unwrap();
    assert!(!svc.enabled);

    services::set_service_enabled(&db.pool, "svc", true)
        .await
        .unwrap();
    let svc = services::get_service_by_slug(&db.pool, "svc")
        .await
        .unwrap()
        .unwrap();
    assert!(svc.enabled);
}

#[tokio::test]
async fn test_count_services() {
    let db = common::TestDb::new().await;

    assert_eq!(services::count_services(&db.pool).await.unwrap(), 0);

    services::insert_service(&db.pool, "a", "A", None)
        .await
        .unwrap();
    services::insert_service(&db.pool, "b", "B", None)
        .await
        .unwrap();

    assert_eq!(services::count_services(&db.pool).await.unwrap(), 2);
}

#[tokio::test]
async fn test_duplicate_slug_fails() {
    let db = common::TestDb::new().await;

    services::insert_service(&db.pool, "dupe", "First", None)
        .await
        .unwrap();

    let result = services::insert_service(&db.pool, "dupe", "Second", None).await;
    assert!(result.is_err());
}
