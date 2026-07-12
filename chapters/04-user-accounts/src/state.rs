//! Server-only shared `#[server]` state, threaded via `axum::Extension`:
//! just the Postgres pool now. Accounts and sessions live in Clerk, not here.

use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
}

impl AppState {
    /// Connect Postgres and run our own migrations.
    pub async fn new() -> Self {
        dotenvy::dotenv().ok();
        let database_url =
            std::env::var("DATABASE_URL").expect("DATABASE_URL must be set (see .env)");

        let pool = PgPoolOptions::new()
            .max_connections(10)
            .connect(&database_url)
            .await
            .expect("failed to connect to postgres");
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .expect("failed to run migrations");

        Self { pool }
    }
}
