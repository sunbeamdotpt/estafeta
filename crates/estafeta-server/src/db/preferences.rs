use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, sqlx::FromRow)]
pub struct UserPreferenceRow {
    pub id: Uuid,
    pub tenant_id: Option<Uuid>,
    pub user_id: String,
    pub global_enabled: bool,
    pub catch_up_mode: String,
    pub sort_mode: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, sqlx::FromRow)]
pub struct UserServicePreferenceRow {
    pub id: Uuid,
    pub tenant_id: Option<Uuid>,
    pub user_id: String,
    pub service_id: Uuid,
    pub enabled: bool,
    pub min_severity: Option<i32>,
}

#[derive(Debug, sqlx::FromRow)]
pub struct UserTypePreferenceRow {
    pub id: Uuid,
    pub tenant_id: Option<Uuid>,
    pub user_id: String,
    pub notification_type_id: Uuid,
    pub enabled: bool,
}

#[derive(Debug, sqlx::FromRow)]
pub struct GlobalPolicyRow {
    pub id: Uuid,
    pub tenant_id: Option<Uuid>,
    pub max_notifications_per_user_per_hour: i32,
    pub max_ttl_seconds: i32,
    pub max_escalations: i32,
    pub rate_limit_per_service_per_second: i32,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

// --- User Preferences ---

pub async fn get_or_create_user_preference(
    pool: &PgPool,
    user_id: &str,
) -> Result<UserPreferenceRow, sqlx::Error> {
    sqlx::query_as::<_, UserPreferenceRow>(
        r#"
        INSERT INTO user_preferences (user_id)
        VALUES ($1)
        ON CONFLICT (user_id) DO UPDATE SET user_id = EXCLUDED.user_id
        RETURNING *
        "#,
    )
    .bind(user_id)
    .fetch_one(pool)
    .await
}

pub async fn update_user_preference(
    pool: &PgPool,
    user_id: &str,
    global_enabled: bool,
    catch_up_mode: &str,
    sort_mode: &str,
) -> Result<UserPreferenceRow, sqlx::Error> {
    sqlx::query_as::<_, UserPreferenceRow>(
        r#"
        INSERT INTO user_preferences (user_id, global_enabled, catch_up_mode, sort_mode)
        VALUES ($1, $2, $3, $4)
        ON CONFLICT (user_id) DO UPDATE
            SET global_enabled = $2, catch_up_mode = $3, sort_mode = $4, updated_at = now()
        RETURNING *
        "#,
    )
    .bind(user_id)
    .bind(global_enabled)
    .bind(catch_up_mode)
    .bind(sort_mode)
    .fetch_one(pool)
    .await
}

// --- Service Preferences ---

pub async fn upsert_service_preference(
    pool: &PgPool,
    user_id: &str,
    service_id: Uuid,
    enabled: bool,
    min_severity: Option<i32>,
) -> Result<UserServicePreferenceRow, sqlx::Error> {
    sqlx::query_as::<_, UserServicePreferenceRow>(
        r#"
        INSERT INTO user_service_preferences (user_id, service_id, enabled, min_severity)
        VALUES ($1, $2, $3, $4)
        ON CONFLICT (user_id, service_id) DO UPDATE
            SET enabled = $3, min_severity = $4
        RETURNING *
        "#,
    )
    .bind(user_id)
    .bind(service_id)
    .bind(enabled)
    .bind(min_severity)
    .fetch_one(pool)
    .await
}

pub async fn delete_service_preference(
    pool: &PgPool,
    user_id: &str,
    service_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM user_service_preferences WHERE user_id = $1 AND service_id = $2")
        .bind(user_id)
        .bind(service_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn list_service_preferences(
    pool: &PgPool,
    user_id: &str,
) -> Result<Vec<UserServicePreferenceRow>, sqlx::Error> {
    sqlx::query_as::<_, UserServicePreferenceRow>(
        "SELECT * FROM user_service_preferences WHERE user_id = $1",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
}

// --- Type Preferences ---

pub async fn upsert_type_preference(
    pool: &PgPool,
    user_id: &str,
    notification_type_id: Uuid,
    enabled: bool,
) -> Result<UserTypePreferenceRow, sqlx::Error> {
    sqlx::query_as::<_, UserTypePreferenceRow>(
        r#"
        INSERT INTO user_type_preferences (user_id, notification_type_id, enabled)
        VALUES ($1, $2, $3)
        ON CONFLICT (user_id, notification_type_id) DO UPDATE
            SET enabled = $3
        RETURNING *
        "#,
    )
    .bind(user_id)
    .bind(notification_type_id)
    .bind(enabled)
    .fetch_one(pool)
    .await
}

pub async fn delete_type_preference(
    pool: &PgPool,
    user_id: &str,
    notification_type_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "DELETE FROM user_type_preferences WHERE user_id = $1 AND notification_type_id = $2",
    )
    .bind(user_id)
    .bind(notification_type_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn list_type_preferences(
    pool: &PgPool,
    user_id: &str,
) -> Result<Vec<UserTypePreferenceRow>, sqlx::Error> {
    sqlx::query_as::<_, UserTypePreferenceRow>(
        "SELECT * FROM user_type_preferences WHERE user_id = $1",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
}

// --- Global Policy ---

pub async fn get_global_policy(pool: &PgPool) -> Result<GlobalPolicyRow, sqlx::Error> {
    sqlx::query_as::<_, GlobalPolicyRow>(
        "SELECT * FROM global_policies ORDER BY created_at LIMIT 1",
    )
    .fetch_one(pool)
    .await
}

pub async fn update_global_policy(
    pool: &PgPool,
    max_notifications_per_user_per_hour: i32,
    max_ttl_seconds: i32,
    max_escalations: i32,
    rate_limit_per_service_per_second: i32,
) -> Result<GlobalPolicyRow, sqlx::Error> {
    sqlx::query_as::<_, GlobalPolicyRow>(
        r#"
        UPDATE global_policies
        SET max_notifications_per_user_per_hour = $1,
            max_ttl_seconds = $2,
            max_escalations = $3,
            rate_limit_per_service_per_second = $4,
            updated_at = now()
        WHERE id = (SELECT id FROM global_policies ORDER BY created_at LIMIT 1)
        RETURNING *
        "#,
    )
    .bind(max_notifications_per_user_per_hour)
    .bind(max_ttl_seconds)
    .bind(max_escalations)
    .bind(rate_limit_per_service_per_second)
    .fetch_one(pool)
    .await
}
