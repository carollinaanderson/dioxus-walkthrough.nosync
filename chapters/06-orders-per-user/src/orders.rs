//! Server-only Postgres store for orders. Every query is now scoped to a
//! `user_id` — this is the whole chapter.

use sqlx::{prelude::FromRow, PgPool};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, FromRow)]
pub struct OrderRow {
    pub id: Uuid,
    pub user_id: String,
    pub item: String,
    pub amount: i64,
    pub status: String,
}

pub async fn insert(
    pool: &PgPool,
    user_id: &str,
    item: &str,
    amount: u32,
) -> Result<OrderRow, sqlx::Error> {
    sqlx::query_as::<_, OrderRow>(
        "INSERT INTO orders (user_id, item, amount) VALUES ($1, $2, $3)
         RETURNING id, user_id, item, amount, status",
    )
    .bind(user_id)
    .bind(item)
    .bind(amount as i64)
    .fetch_one(pool)
    .await
}

pub async fn list_for_user(pool: &PgPool, user_id: &str) -> Result<Vec<OrderRow>, sqlx::Error> {
    sqlx::query_as::<_, OrderRow>(
        "SELECT id, user_id, item, amount, status FROM orders
         WHERE user_id = $1 ORDER BY created_at DESC",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
}

pub async fn get_for_user(
    pool: &PgPool,
    user_id: &str,
    id: Uuid,
) -> Result<Option<OrderRow>, sqlx::Error> {
    sqlx::query_as::<_, OrderRow>(
        "SELECT id, user_id, item, amount, status FROM orders WHERE id = $1 AND user_id = $2",
    )
    .bind(id)
    .bind(user_id)
    .fetch_optional(pool)
    .await
}
