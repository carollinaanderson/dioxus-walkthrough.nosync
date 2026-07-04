//! Server-only Postgres store for order business-data. Reuses duroxide-pg's
//! `PgPool` (see docs/API-NOTES.md). Schema is applied via the sqlx migration
//! in `migrations/`.

use std::sync::OnceLock;

use sqlx::{migrate::MigrateError, prelude::FromRow, PgPool};

static POOL: OnceLock<PgPool> = OnceLock::new();

#[derive(Debug, Clone, PartialEq, FromRow)]
pub struct OrderRow {
    pub instance_id: String,
    pub item: String,
    pub amount: i64,
}

/// Store the (shared) pool and apply pending sqlx migrations.
pub async fn init(pool: PgPool) -> Result<(), MigrateError> {
    sqlx::migrate!("./migrations").run(&pool).await?;
    let _ = POOL.set(pool);
    Ok(())
}

/// Clone of the stored pool. Panics if `init` has not run.
pub fn pool() -> PgPool {
    POOL.get().expect("orders::init not called").clone()
}

pub async fn insert(instance_id: &str, item: &str, amount: u32) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO orders (instance_id, item, amount) VALUES ($1, $2, $3)")
        .bind(instance_id)
        .bind(item)
        .bind(amount as i64)
        .execute(&pool())
        .await?;
    Ok(())
}

pub async fn list() -> Result<Vec<OrderRow>, sqlx::Error> {
    sqlx::query_as::<_, OrderRow>(
        "SELECT instance_id, item, amount FROM orders ORDER BY created_at DESC",
    )
    .fetch_all(&pool())
    .await
}

pub async fn get(instance_id: &str) -> Result<OrderRow, sqlx::Error> {
    sqlx::query_as::<_, OrderRow>(
        "SELECT instance_id, item, amount FROM orders WHERE instance_id = $1",
    )
    .bind(instance_id)
    .fetch_one(&pool())
    .await
}

// The orders store is exercised end-to-end (insert/get/list) against real
// Postgres by `workflow::tests::postgres_order_lifecycle`, which owns the single
// runtime + pool. It is not tested standalone here because a sqlx pool is bound
// to the tokio runtime that created it and cannot be shared across separate
// `#[tokio::test]` functions.
