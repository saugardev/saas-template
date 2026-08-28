use anyhow::Context;
use sqlx::{PgPool, postgres::PgPoolOptions};
use std::time::Duration;

pub type Database = PgPool;

pub async fn connect(database_url: &str) -> anyhow::Result<Database> {
    PgPoolOptions::new()
        .min_connections(1)
        .max_connections(10)
        .acquire_timeout(Duration::from_secs(10))
        .connect(database_url)
        .await
        .context("connect to PostgreSQL")
}

pub async fn migrate(pool: &Database) -> anyhow::Result<()> {
    sqlx::migrate!("./migrations")
        .run(pool)
        .await
        .context("apply database migrations")
}

pub async fn ready(pool: &Database) -> bool {
    sqlx::query_scalar::<_, i32>("SELECT 1")
        .fetch_one(pool)
        .await
        .is_ok()
}
