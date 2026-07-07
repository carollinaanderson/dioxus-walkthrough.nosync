//! Server-only Postgres store for user accounts.

use sqlx::{prelude::FromRow, PgPool};
use uuid::Uuid;

#[derive(Debug, Clone, FromRow)]
pub struct UserRow {
    pub id: Uuid,
    pub username: String,
    pub password_hash: String,
}

pub async fn insert(
    pool: &PgPool,
    username: &str,
    password_hash: &str,
) -> Result<UserRow, sqlx::Error> {
    sqlx::query_as::<_, UserRow>(
        "INSERT INTO users (username, password_hash) VALUES ($1, $2)
         RETURNING id, username, password_hash",
    )
    .bind(username)
    .bind(password_hash)
    .fetch_one(pool)
    .await
}

pub async fn find_by_username(
    pool: &PgPool,
    username: &str,
) -> Result<Option<UserRow>, sqlx::Error> {
    sqlx::query_as::<_, UserRow>(
        "SELECT id, username, password_hash FROM users WHERE username = $1",
    )
    .bind(username)
    .fetch_optional(pool)
    .await
}

pub async fn find_by_id(pool: &PgPool, id: Uuid) -> Result<Option<UserRow>, sqlx::Error> {
    sqlx::query_as::<_, UserRow>("SELECT id, username, password_hash FROM users WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await
}
