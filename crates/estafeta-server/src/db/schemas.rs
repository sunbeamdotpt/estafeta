use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, sqlx::FromRow)]
pub struct NotificationTypeRow {
    pub id: Uuid,
    pub tenant_id: Option<Uuid>,
    pub service_id: Uuid,
    pub type_key: String,
    pub display_name: String,
    pub description: Option<String>,
    pub json_schema: serde_json::Value,
    pub default_channels: Vec<String>,
    pub default_ttl_seconds: Option<i32>,
    pub escalation_interval_seconds: Option<i32>,
    pub max_escalations: i32,
    pub enabled: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, sqlx::FromRow)]
pub struct NotificationLevelRow {
    pub id: Uuid,
    pub tenant_id: Option<Uuid>,
    pub service_id: Uuid,
    pub key: String,
    pub display_name: String,
    pub severity: i32,
    pub color: Option<String>,
    pub icon: Option<String>,
}

pub async fn insert_notification_type(
    pool: &PgPool,
    service_id: Uuid,
    type_key: &str,
    display_name: &str,
    description: Option<&str>,
    json_schema: &serde_json::Value,
    default_channels: &[String],
    default_ttl_seconds: Option<i32>,
    escalation_interval_seconds: Option<i32>,
    max_escalations: i32,
) -> Result<NotificationTypeRow, sqlx::Error> {
    sqlx::query_as::<_, NotificationTypeRow>(
        r#"
        INSERT INTO notification_types
            (service_id, type_key, display_name, description, json_schema,
             default_channels, default_ttl_seconds, escalation_interval_seconds, max_escalations)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        RETURNING *
        "#,
    )
    .bind(service_id)
    .bind(type_key)
    .bind(display_name)
    .bind(description)
    .bind(json_schema)
    .bind(default_channels)
    .bind(default_ttl_seconds)
    .bind(escalation_interval_seconds)
    .bind(max_escalations)
    .fetch_one(pool)
    .await
}

pub async fn update_notification_type(
    pool: &PgPool,
    service_id: Uuid,
    type_key: &str,
    display_name: &str,
    description: Option<&str>,
    json_schema: &serde_json::Value,
    default_channels: &[String],
    default_ttl_seconds: Option<i32>,
    escalation_interval_seconds: Option<i32>,
    max_escalations: i32,
    enabled: bool,
) -> Result<NotificationTypeRow, sqlx::Error> {
    sqlx::query_as::<_, NotificationTypeRow>(
        r#"
        UPDATE notification_types
        SET display_name = $3, description = $4, json_schema = $5,
            default_channels = $6, default_ttl_seconds = $7,
            escalation_interval_seconds = $8, max_escalations = $9,
            enabled = $10, updated_at = now()
        WHERE service_id = $1 AND type_key = $2
        RETURNING *
        "#,
    )
    .bind(service_id)
    .bind(type_key)
    .bind(display_name)
    .bind(description)
    .bind(json_schema)
    .bind(default_channels)
    .bind(default_ttl_seconds)
    .bind(escalation_interval_seconds)
    .bind(max_escalations)
    .bind(enabled)
    .fetch_one(pool)
    .await
}

pub async fn get_notification_type(
    pool: &PgPool,
    service_id: Uuid,
    type_key: &str,
) -> Result<Option<NotificationTypeRow>, sqlx::Error> {
    sqlx::query_as::<_, NotificationTypeRow>(
        "SELECT * FROM notification_types WHERE service_id = $1 AND type_key = $2",
    )
    .bind(service_id)
    .bind(type_key)
    .fetch_optional(pool)
    .await
}

pub async fn list_notification_types(
    pool: &PgPool,
    service_id: Uuid,
    limit: i64,
    offset: i64,
) -> Result<Vec<NotificationTypeRow>, sqlx::Error> {
    sqlx::query_as::<_, NotificationTypeRow>(
        "SELECT * FROM notification_types WHERE service_id = $1 ORDER BY type_key LIMIT $2 OFFSET $3",
    )
    .bind(service_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
}

pub async fn insert_notification_level(
    pool: &PgPool,
    service_id: Uuid,
    key: &str,
    display_name: &str,
    severity: i32,
    color: Option<&str>,
    icon: Option<&str>,
) -> Result<NotificationLevelRow, sqlx::Error> {
    sqlx::query_as::<_, NotificationLevelRow>(
        r#"
        INSERT INTO notification_levels (service_id, key, display_name, severity, color, icon)
        VALUES ($1, $2, $3, $4, $5, $6)
        RETURNING *
        "#,
    )
    .bind(service_id)
    .bind(key)
    .bind(display_name)
    .bind(severity)
    .bind(color)
    .bind(icon)
    .fetch_one(pool)
    .await
}

pub async fn update_notification_level(
    pool: &PgPool,
    service_id: Uuid,
    key: &str,
    display_name: &str,
    severity: i32,
    color: Option<&str>,
    icon: Option<&str>,
) -> Result<NotificationLevelRow, sqlx::Error> {
    sqlx::query_as::<_, NotificationLevelRow>(
        r#"
        UPDATE notification_levels
        SET display_name = $3, severity = $4, color = $5, icon = $6
        WHERE service_id = $1 AND key = $2
        RETURNING *
        "#,
    )
    .bind(service_id)
    .bind(key)
    .bind(display_name)
    .bind(severity)
    .bind(color)
    .bind(icon)
    .fetch_one(pool)
    .await
}

pub async fn list_notification_levels(
    pool: &PgPool,
    service_id: Uuid,
) -> Result<Vec<NotificationLevelRow>, sqlx::Error> {
    sqlx::query_as::<_, NotificationLevelRow>(
        "SELECT * FROM notification_levels WHERE service_id = $1 ORDER BY severity DESC",
    )
    .bind(service_id)
    .fetch_all(pool)
    .await
}

pub async fn get_notification_level(
    pool: &PgPool,
    service_id: Uuid,
    key: &str,
) -> Result<Option<NotificationLevelRow>, sqlx::Error> {
    sqlx::query_as::<_, NotificationLevelRow>(
        "SELECT * FROM notification_levels WHERE service_id = $1 AND key = $2",
    )
    .bind(service_id)
    .bind(key)
    .fetch_optional(pool)
    .await
}
