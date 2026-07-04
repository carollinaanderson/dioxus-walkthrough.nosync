//! Server-only Postgres store for order business-data. Reuses duroxide-pg's
//! `PgPool` (see docs/API-NOTES.md). Schema is applied via the sqlx migration
//! in `migrations/`.

use std::sync::OnceLock;

use sqlx::{PgPool, Row};

static POOL: OnceLock<PgPool> = OnceLock::new();
static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

#[derive(Debug, Clone, PartialEq)]
pub struct OrderRow {
    pub instance_id: String,
    pub item: String,
    pub amount: u32,
}

/// Store the (shared) pool and apply pending sqlx migrations.
pub async fn init(pool: PgPool) -> Result<(), String> {
    MIGRATOR.run(&pool).await.map_err(|e| e.to_string())?;
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

fn row_from(r: &sqlx::postgres::PgRow) -> OrderRow {
    OrderRow {
        instance_id: r.get("instance_id"),
        item: r.get("item"),
        amount: r.get::<i64, _>("amount") as u32,
    }
}

pub async fn list() -> Result<Vec<OrderRow>, sqlx::Error> {
    let rows = sqlx::query("SELECT instance_id, item, amount FROM orders ORDER BY created_at DESC")
        .fetch_all(&pool())
        .await?;
    Ok(rows.iter().map(row_from).collect())
}

pub async fn get(instance_id: &str) -> Result<Option<OrderRow>, sqlx::Error> {
    let row = sqlx::query("SELECT instance_id, item, amount FROM orders WHERE instance_id = $1")
        .bind(instance_id)
        .fetch_optional(&pool())
        .await?;
    Ok(row.as_ref().map(row_from))
}

// The orders store is exercised end-to-end (insert/get/list) against real
// Postgres by `workflow::tests::postgres_order_lifecycle`, which owns the single
// runtime + pool. It is not tested standalone here because a sqlx pool is bound
// to the tokio runtime that created it and cannot be shared across separate
// `#[tokio::test]` functions.
