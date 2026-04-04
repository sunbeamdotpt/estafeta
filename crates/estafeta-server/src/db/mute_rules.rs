use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, sqlx::FromRow)]
pub struct MuteRuleRow {
    pub id: Uuid,
    pub tenant_id: Option<Uuid>,
    pub user_id: String,
    pub service_id: Option<Uuid>,
    pub notification_type_id: Option<Uuid>,
    pub muted_until: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

pub async fn insert_mute_rule(
    pool: &PgPool,
    user_id: &str,
    service_id: Option<Uuid>,
    notification_type_id: Option<Uuid>,
    muted_until: Option<DateTime<Utc>>,
) -> Result<MuteRuleRow, sqlx::Error> {
    sqlx::query_as::<_, MuteRuleRow>(
        r#"
        INSERT INTO mute_rules (user_id, service_id, notification_type_id, muted_until)
        VALUES ($1, $2, $3, $4)
        RETURNING *
        "#,
    )
    .bind(user_id)
    .bind(service_id)
    .bind(notification_type_id)
    .bind(muted_until)
    .fetch_one(pool)
    .await
}

pub async fn delete_mute_rule(
    pool: &PgPool,
    user_id: &str,
    rule_id: Uuid,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query("DELETE FROM mute_rules WHERE id = $1 AND user_id = $2")
        .bind(rule_id)
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn list_mute_rules(
    pool: &PgPool,
    user_id: &str,
) -> Result<Vec<MuteRuleRow>, sqlx::Error> {
    sqlx::query_as::<_, MuteRuleRow>(
        "SELECT * FROM mute_rules WHERE user_id = $1 ORDER BY created_at DESC",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
}

/// Get active mute rules for a user (not expired).
pub async fn list_active_mute_rules(
    pool: &PgPool,
    user_id: &str,
) -> Result<Vec<MuteRuleRow>, sqlx::Error> {
    sqlx::query_as::<_, MuteRuleRow>(
        r#"
        SELECT * FROM mute_rules
        WHERE user_id = $1
          AND (muted_until IS NULL OR muted_until > now())
        "#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
}
