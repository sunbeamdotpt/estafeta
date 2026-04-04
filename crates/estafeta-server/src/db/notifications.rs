use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, sqlx::FromRow, Clone)]
pub struct NotificationRow {
    pub id: Uuid,
    pub tenant_id: Option<Uuid>,
    pub service_id: Uuid,
    pub notification_type_id: Uuid,
    pub level_id: Option<Uuid>,
    pub recipient_user_id: String,
    pub state: String,
    pub payload: serde_json::Value,
    pub group_key: Option<String>,
    pub idempotency_key: Option<String>,
    pub metadata: serde_json::Value,
    pub snoozed_until: Option<DateTime<Utc>>,
    pub expires_at: Option<DateTime<Utc>>,
    pub next_escalation_at: Option<DateTime<Utc>>,
    pub escalation_count: i32,
    pub read_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub struct InsertNotification {
    pub id: Uuid,
    pub service_id: Uuid,
    pub notification_type_id: Uuid,
    pub level_id: Option<Uuid>,
    pub recipient_user_id: String,
    pub state: String,
    pub payload: serde_json::Value,
    pub group_key: Option<String>,
    pub idempotency_key: Option<String>,
    pub metadata: serde_json::Value,
    pub expires_at: Option<DateTime<Utc>>,
    pub next_escalation_at: Option<DateTime<Utc>>,
}

pub async fn insert_notification(
    pool: &PgPool,
    n: &InsertNotification,
) -> Result<NotificationRow, sqlx::Error> {
    sqlx::query_as::<_, NotificationRow>(
        r#"
        INSERT INTO notifications
            (id, service_id, notification_type_id, level_id, recipient_user_id,
             state, payload, group_key, idempotency_key, metadata, expires_at, next_escalation_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
        RETURNING *
        "#,
    )
    .bind(n.id)
    .bind(n.service_id)
    .bind(n.notification_type_id)
    .bind(n.level_id)
    .bind(&n.recipient_user_id)
    .bind(&n.state)
    .bind(&n.payload)
    .bind(&n.group_key)
    .bind(&n.idempotency_key)
    .bind(&n.metadata)
    .bind(n.expires_at)
    .bind(n.next_escalation_at)
    .fetch_one(pool)
    .await
}

pub async fn get_notification(
    pool: &PgPool,
    id: Uuid,
) -> Result<Option<NotificationRow>, sqlx::Error> {
    sqlx::query_as::<_, NotificationRow>("SELECT * FROM notifications WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await
}

pub async fn list_notifications(
    pool: &PgPool,
    user_id: &str,
    states: &[String],
    service_ids: &[Uuid],
    type_ids: &[Uuid],
    group_key: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<Vec<NotificationRow>, sqlx::Error> {
    sqlx::query_as::<_, NotificationRow>(
        r#"
        SELECT * FROM notifications
        WHERE recipient_user_id = $1
          AND ($2::text[] IS NULL OR state = ANY($2))
          AND ($3::uuid[] IS NULL OR service_id = ANY($3))
          AND ($4::uuid[] IS NULL OR notification_type_id = ANY($4))
          AND ($5::text IS NULL OR group_key = $5)
        ORDER BY created_at DESC
        LIMIT $6 OFFSET $7
        "#,
    )
    .bind(user_id)
    .bind(if states.is_empty() { None } else { Some(states) })
    .bind(if service_ids.is_empty() { None } else { Some(service_ids) })
    .bind(if type_ids.is_empty() { None } else { Some(type_ids) })
    .bind(group_key)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
}

pub async fn count_unread(
    pool: &PgPool,
    user_id: &str,
    service_ids: &[Uuid],
) -> Result<i64, sqlx::Error> {
    let row: (i64,) = sqlx::query_as(
        r#"
        SELECT COUNT(*) FROM notifications
        WHERE recipient_user_id = $1
          AND state = 'unread'
          AND ($2::uuid[] IS NULL OR service_id = ANY($2))
        "#,
    )
    .bind(user_id)
    .bind(if service_ids.is_empty() { None } else { Some(service_ids) })
    .fetch_one(pool)
    .await?;
    Ok(row.0)
}

pub async fn mark_read(
    pool: &PgPool,
    user_id: &str,
    ids: &[Uuid],
) -> Result<u64, sqlx::Error> {
    let result = sqlx::query(
        r#"
        UPDATE notifications
        SET state = 'read', read_at = now(), updated_at = now()
        WHERE recipient_user_id = $1 AND id = ANY($2) AND state = 'unread'
        "#,
    )
    .bind(user_id)
    .bind(ids)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

pub async fn mark_unread(
    pool: &PgPool,
    user_id: &str,
    ids: &[Uuid],
) -> Result<u64, sqlx::Error> {
    let result = sqlx::query(
        r#"
        UPDATE notifications
        SET state = 'unread', read_at = NULL, updated_at = now()
        WHERE recipient_user_id = $1 AND id = ANY($2) AND state = 'read'
        "#,
    )
    .bind(user_id)
    .bind(ids)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

pub async fn snooze(
    pool: &PgPool,
    user_id: &str,
    id: Uuid,
    until: DateTime<Utc>,
) -> Result<u64, sqlx::Error> {
    let result = sqlx::query(
        r#"
        UPDATE notifications
        SET state = 'snoozed', snoozed_until = $3, updated_at = now()
        WHERE recipient_user_id = $1 AND id = $2 AND state IN ('unread', 'read')
        "#,
    )
    .bind(user_id)
    .bind(id)
    .bind(until)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

pub async fn dismiss(
    pool: &PgPool,
    user_id: &str,
    ids: &[Uuid],
) -> Result<u64, sqlx::Error> {
    let result = sqlx::query(
        r#"
        UPDATE notifications
        SET state = 'dismissed', updated_at = now()
        WHERE recipient_user_id = $1 AND id = ANY($2)
          AND state IN ('unread', 'read', 'snoozed')
        "#,
    )
    .bind(user_id)
    .bind(ids)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

/// Wake snoozed notifications whose snooze time has passed.
/// Returns IDs that were woken for real-time event publishing.
pub async fn wake_snoozed(pool: &PgPool, batch_size: i64) -> Result<Vec<NotificationRow>, sqlx::Error> {
    sqlx::query_as::<_, NotificationRow>(
        r#"
        UPDATE notifications
        SET state = 'unread', snoozed_until = NULL, updated_at = now()
        WHERE id IN (
            SELECT id FROM notifications
            WHERE state = 'snoozed' AND snoozed_until <= now()
            ORDER BY snoozed_until
            LIMIT $1
            FOR UPDATE SKIP LOCKED
        )
        RETURNING *
        "#,
    )
    .bind(batch_size)
    .fetch_all(pool)
    .await
}

/// Expire notifications past their TTL.
pub async fn expire_notifications(pool: &PgPool, batch_size: i64) -> Result<Vec<NotificationRow>, sqlx::Error> {
    sqlx::query_as::<_, NotificationRow>(
        r#"
        UPDATE notifications
        SET state = 'expired', updated_at = now()
        WHERE id IN (
            SELECT id FROM notifications
            WHERE expires_at <= now() AND state IN ('unread', 'read')
            ORDER BY expires_at
            LIMIT $1
            FOR UPDATE SKIP LOCKED
        )
        RETURNING *
        "#,
    )
    .bind(batch_size)
    .fetch_all(pool)
    .await
}

/// Get notifications due for escalation.
pub async fn get_escalation_due(pool: &PgPool, batch_size: i64) -> Result<Vec<NotificationRow>, sqlx::Error> {
    sqlx::query_as::<_, NotificationRow>(
        r#"
        SELECT * FROM notifications
        WHERE state = 'unread'
          AND next_escalation_at IS NOT NULL
          AND next_escalation_at <= now()
        ORDER BY next_escalation_at
        LIMIT $1
        FOR UPDATE SKIP LOCKED
        "#,
    )
    .bind(batch_size)
    .fetch_all(pool)
    .await
}

/// Update escalation state after re-delivery.
pub async fn update_escalation(
    pool: &PgPool,
    id: Uuid,
    new_escalation_count: i32,
    next_escalation_at: Option<DateTime<Utc>>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        UPDATE notifications
        SET escalation_count = $2, next_escalation_at = $3, updated_at = now()
        WHERE id = $1
        "#,
    )
    .bind(id)
    .bind(new_escalation_count)
    .bind(next_escalation_at)
    .execute(pool)
    .await?;
    Ok(())
}

/// Count unread grouped by service_id for a user.
pub async fn count_unread_by_service(
    pool: &PgPool,
    user_id: &str,
) -> Result<Vec<(Uuid, i64)>, sqlx::Error> {
    sqlx::query_as::<_, (Uuid, i64)>(
        r#"
        SELECT service_id, COUNT(*) as count
        FROM notifications
        WHERE recipient_user_id = $1 AND state = 'unread'
        GROUP BY service_id
        "#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
}
