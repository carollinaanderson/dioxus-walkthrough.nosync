//! Server-only Postgres store for orders. Still no `user_id` column — the
//! orders page is now login-protected, but every logged-in user still sees
//! the same global list. Chapter 6 scopes this per user.

use sqlx::PgPool;
use uuid::Uuid;

#[derive(sqlx::FromRow)]
pub struct OrderRow {
    pub id: Uuid,
    pub item: String,
    pub amount: i64,
    pub status: String,
}

pub async fn insert(pool: &PgPool, item: &str, amount: u32) -> Result<OrderRow, sqlx::Error> {
    sqlx::query_as::<_, OrderRow>(
        "INSERT INTO orders (item, amount) VALUES ($1, $2) RETURNING id, item, amount, status",
    )
    .bind(item)
    .bind(amount as i64)
    .fetch_one(pool)
    .await
}

pub async fn list(pool: &PgPool) -> Result<Vec<OrderRow>, sqlx::Error> {
    sqlx::query_as::<_, OrderRow>(
        "SELECT id, item, amount, status FROM orders ORDER BY created_at DESC",
    )
    .fetch_all(pool)
    .await
}
