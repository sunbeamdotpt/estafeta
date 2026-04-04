use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, sqlx::FromRow)]
pub struct ServiceRow {
    pub id: Uuid,
    pub tenant_id: Option<Uuid>,
    pub slug: String,
    pub display_name: String,
    pub description: Option<String>,
    pub enabled: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

pub async fn insert_service(
    pool: &PgPool,
    slug: &str,
    display_name: &str,
    description: Option<&str>,
) -> Result<ServiceRow, sqlx::Error> {
    sqlx::query_as::<_, ServiceRow>(
        r#"
        INSERT INTO services (slug, display_name, description)
        VALUES ($1, $2, $3)
        RETURNING *
        "#,
    )
    .bind(slug)
    .bind(display_name)
    .bind(description)
    .fetch_one(pool)
    .await
}

pub async fn get_service_by_slug(
    pool: &PgPool,
    slug: &str,
) -> Result<Option<ServiceRow>, sqlx::Error> {
    sqlx::query_as::<_, ServiceRow>("SELECT * FROM services WHERE slug = $1")
        .bind(slug)
        .fetch_optional(pool)
        .await
}

pub async fn list_services(
    pool: &PgPool,
    limit: i64,
    offset: i64,
) -> Result<Vec<ServiceRow>, sqlx::Error> {
    sqlx::query_as::<_, ServiceRow>(
        "SELECT * FROM services ORDER BY created_at DESC LIMIT $1 OFFSET $2",
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
}

pub async fn update_service(
    pool: &PgPool,
    slug: &str,
    display_name: &str,
    description: Option<&str>,
) -> Result<ServiceRow, sqlx::Error> {
    sqlx::query_as::<_, ServiceRow>(
        r#"
        UPDATE services
        SET display_name = $2, description = $3, updated_at = now()
        WHERE slug = $1
        RETURNING *
        "#,
    )
    .bind(slug)
    .bind(display_name)
    .bind(description)
    .fetch_one(pool)
    .await
}

pub async fn set_service_enabled(
    pool: &PgPool,
    slug: &str,
    enabled: bool,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE services SET enabled = $2, updated_at = now() WHERE slug = $1")
        .bind(slug)
        .bind(enabled)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn count_services(pool: &PgPool) -> Result<i64, sqlx::Error> {
    let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM services")
        .fetch_one(pool)
        .await?;
    Ok(row.0)
}
