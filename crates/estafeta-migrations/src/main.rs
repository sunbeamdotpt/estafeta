use sqlx::postgres::PgPoolOptions;
use std::path::Path;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let database_url =
        std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");

    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&database_url)
        .await?;

    let migrations_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");

    sqlx::migrate::Migrator::new(migrations_path)
        .await?
        .run(&pool)
        .await?;

    println!("Migrations applied successfully");
    Ok(())
}
