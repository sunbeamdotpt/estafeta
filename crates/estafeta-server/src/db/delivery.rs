use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, sqlx::FromRow, Clone)]
pub struct DeliveryAttemptRow {
    pub id: Uuid,
    pub tenant_id: Option<Uuid>,
    pub notification_id: Uuid,
    pub channel: String,
    pub status: String,
    pub attempt_number: i32,
    pub next_retry_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    pub external_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub async fn insert_delivery_attempt(
    pool: &PgPool,
    notification_id: Uuid,
    channel: &str,
) -> Result<DeliveryAttemptRow, sqlx::Error> {
    sqlx::query_as::<_, DeliveryAttemptRow>(
        r#"
        INSERT INTO delivery_attempts (notification_id, channel)
        VALUES ($1, $2)
        RETURNING *
        "#,
    )
    .bind(notification_id)
    .bind(channel)
    .fetch_one(pool)
    .await
}

pub async fn get_delivery_attempt(
    pool: &PgPool,
    id: Uuid,
) -> Result<Option<DeliveryAttemptRow>, sqlx::Error> {
    sqlx::query_as::<_, DeliveryAttemptRow>(
        "SELECT * FROM delivery_attempts WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
}

pub async fn update_delivery_status(
    pool: &PgPool,
    id: Uuid,
    status: &str,
    external_id: Option<&str>,
    last_error: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        UPDATE delivery_attempts
        SET status = $2, external_id = $3, last_error = $4, updated_at = now()
        WHERE id = $1
        "#,
    )
    .bind(id)
    .bind(status)
    .bind(external_id)
    .bind(last_error)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn schedule_retry(
    pool: &PgPool,
    id: Uuid,
    attempt_number: i32,
    next_retry_at: DateTime<Utc>,
    last_error: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        UPDATE delivery_attempts
        SET attempt_number = $2, next_retry_at = $3, last_error = $4,
            status = 'pending', updated_at = now()
        WHERE id = $1
        "#,
    )
    .bind(id)
    .bind(attempt_number)
    .bind(next_retry_at)
    .bind(last_error)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn list_delivery_attempts_for_notification(
    pool: &PgPool,
    notification_id: Uuid,
) -> Result<Vec<DeliveryAttemptRow>, sqlx::Error> {
    sqlx::query_as::<_, DeliveryAttemptRow>(
        "SELECT * FROM delivery_attempts WHERE notification_id = $1 ORDER BY created_at",
    )
    .bind(notification_id)
    .fetch_all(pool)
    .await
}
