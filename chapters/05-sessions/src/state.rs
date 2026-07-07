//! Server-only shared state, threaded into `#[server]` functions via
//! `axum::Extension` instead of process-global statics.

use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
}

impl AppState {
    /// Connect Postgres and run migrations. Called once, at server boot.
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
