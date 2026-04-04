use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, sqlx::FromRow)]
pub struct UserPreferenceRow {
    pub id: Uuid,
    pub tenant_id: Option<Uuid>,
    pub user_id: String,
    pub global_enabled: bool,
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
    pub channels: Vec<String>,
}

#[derive(Debug, sqlx::FromRow)]
pub struct UserTypePreferenceRow {
    pub id: Uuid,
    pub tenant_id: Option<Uuid>,
    pub user_id: String,
    pub notification_type_id: Uuid,
    pub enabled: bool,
    pub channels: Option<Vec<String>>,
}

#[derive(Debug, sqlx::FromRow)]
pub struct UserChannelConfigRow {
    pub id: Uuid,
    pub tenant_id: Option<Uuid>,
    pub user_id: String,
    pub email_address: Option<String>,
    pub phone_number: Option<String>,
    pub webhook_url: Option<String>,
    pub webhook_secret: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, sqlx::FromRow)]
pub struct UserDeviceRow {
    pub id: Uuid,
    pub tenant_id: Option<Uuid>,
    pub user_id: String,
    pub device_id: String,
    pub platform: String,
    pub push_token: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, sqlx::FromRow)]
pub struct GlobalPolicyRow {
    pub id: Uuid,
    pub tenant_id: Option<Uuid>,
    pub max_notifications_per_user_per_hour: i32,
    pub max_ttl_seconds: i32,
    pub max_escalations: i32,
    pub default_channels: Vec<String>,
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
) -> Result<UserPreferenceRow, sqlx::Error> {
    sqlx::query_as::<_, UserPreferenceRow>(
        r#"
        INSERT INTO user_preferences (user_id, global_enabled)
        VALUES ($1, $2)
        ON CONFLICT (user_id) DO UPDATE SET global_enabled = $2, updated_at = now()
        RETURNING *
        "#,
    )
    .bind(user_id)
    .bind(global_enabled)
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
    channels: &[String],
) -> Result<UserServicePreferenceRow, sqlx::Error> {
    sqlx::query_as::<_, UserServicePreferenceRow>(
        r#"
        INSERT INTO user_service_preferences (user_id, service_id, enabled, min_severity, channels)
        VALUES ($1, $2, $3, $4, $5)
        ON CONFLICT (user_id, service_id) DO UPDATE
            SET enabled = $3, min_severity = $4, channels = $5
        RETURNING *
        "#,
    )
    .bind(user_id)
    .bind(service_id)
    .bind(enabled)
    .bind(min_severity)
    .bind(channels)
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
    channels: Option<&[String]>,
) -> Result<UserTypePreferenceRow, sqlx::Error> {
    sqlx::query_as::<_, UserTypePreferenceRow>(
        r#"
        INSERT INTO user_type_preferences (user_id, notification_type_id, enabled, channels)
        VALUES ($1, $2, $3, $4)
        ON CONFLICT (user_id, notification_type_id) DO UPDATE
            SET enabled = $3, channels = $4
        RETURNING *
        "#,
    )
    .bind(user_id)
    .bind(notification_type_id)
    .bind(enabled)
    .bind(channels)
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

// --- Channel Config ---

pub async fn upsert_channel_config(
    pool: &PgPool,
    user_id: &str,
    email_address: Option<&str>,
    phone_number: Option<&str>,
    webhook_url: Option<&str>,
    webhook_secret: Option<&str>,
) -> Result<UserChannelConfigRow, sqlx::Error> {
    sqlx::query_as::<_, UserChannelConfigRow>(
        r#"
        INSERT INTO user_channel_configs (user_id, email_address, phone_number, webhook_url, webhook_secret)
        VALUES ($1, $2, $3, $4, $5)
        ON CONFLICT (user_id) DO UPDATE
            SET email_address = COALESCE($2, user_channel_configs.email_address),
                phone_number = COALESCE($3, user_channel_configs.phone_number),
                webhook_url = COALESCE($4, user_channel_configs.webhook_url),
                webhook_secret = COALESCE($5, user_channel_configs.webhook_secret),
                updated_at = now()
        RETURNING *
        "#,
    )
    .bind(user_id)
    .bind(email_address)
    .bind(phone_number)
    .bind(webhook_url)
    .bind(webhook_secret)
    .fetch_one(pool)
    .await
}

pub async fn get_channel_config(
    pool: &PgPool,
    user_id: &str,
) -> Result<Option<UserChannelConfigRow>, sqlx::Error> {
    sqlx::query_as::<_, UserChannelConfigRow>(
        "SELECT * FROM user_channel_configs WHERE user_id = $1",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
}

// --- Devices ---

pub async fn upsert_device(
    pool: &PgPool,
    user_id: &str,
    device_id: &str,
    platform: &str,
    push_token: &str,
) -> Result<UserDeviceRow, sqlx::Error> {
    sqlx::query_as::<_, UserDeviceRow>(
        r#"
        INSERT INTO user_devices (user_id, device_id, platform, push_token)
        VALUES ($1, $2, $3, $4)
        ON CONFLICT (user_id, device_id) DO UPDATE
            SET platform = $3, push_token = $4, updated_at = now()
        RETURNING *
        "#,
    )
    .bind(user_id)
    .bind(device_id)
    .bind(platform)
    .bind(push_token)
    .fetch_one(pool)
    .await
}

pub async fn delete_device(
    pool: &PgPool,
    user_id: &str,
    device_id: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM user_devices WHERE user_id = $1 AND device_id = $2")
        .bind(user_id)
        .bind(device_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn list_devices(
    pool: &PgPool,
    user_id: &str,
) -> Result<Vec<UserDeviceRow>, sqlx::Error> {
    sqlx::query_as::<_, UserDeviceRow>("SELECT * FROM user_devices WHERE user_id = $1")
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
    default_channels: &[String],
    rate_limit_per_service_per_second: i32,
) -> Result<GlobalPolicyRow, sqlx::Error> {
    sqlx::query_as::<_, GlobalPolicyRow>(
        r#"
        UPDATE global_policies
        SET max_notifications_per_user_per_hour = $1,
            max_ttl_seconds = $2,
            max_escalations = $3,
            default_channels = $4,
            rate_limit_per_service_per_second = $5,
            updated_at = now()
        WHERE id = (SELECT id FROM global_policies ORDER BY created_at LIMIT 1)
        RETURNING *
        "#,
    )
    .bind(max_notifications_per_user_per_hour)
    .bind(max_ttl_seconds)
    .bind(max_escalations)
    .bind(default_channels)
    .bind(rate_limit_per_service_per_second)
    .fetch_one(pool)
    .await
}
