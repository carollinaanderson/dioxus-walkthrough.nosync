//! Server-only shared `#[server]` state, threaded via `axum::Extension`:
//! the Postgres pool and the better-auth.rs instance (which shares that
//! same pool for its own tables).

use std::sync::Arc;

use better_auth::adapters::SqlxAdapter;
use better_auth::plugins::{EmailPasswordPlugin, SessionManagementPlugin};
use better_auth::{AuthBuilder, AuthConfig, BetterAuth};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub auth: Arc<BetterAuth<SqlxAdapter>>,
}

impl AppState {
    /// Connect Postgres, run our own migrations, then build better-auth.rs
    /// on top of the same pool.
    pub async fn new() -> Self {
        dotenvy::dotenv().ok();
        let database_url =
            std::env::var("DATABASE_URL").expect("DATABASE_URL must be set (see .env)");
        let secret =
            std::env::var("BETTER_AUTH_SECRET").expect("BETTER_AUTH_SECRET must be set (see .env)");

        let pool = PgPoolOptions::new()
            .max_connections(10)
            .connect(&database_url)
            .await
            .expect("failed to connect to postgres");
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .expect("failed to run migrations");

        let config = AuthConfig::new(secret).base_url("http://localhost:8080");
        let auth = AuthBuilder::new(config)
            .database(SqlxAdapter::from_pool(pool.clone()))
            .plugin(
                EmailPasswordPlugin::new()
                    .enable_signup(true)
                    .password_min_length(8),
            )
            .plugin(SessionManagementPlugin::new())
            .build()
            .await
            .expect("failed to build better-auth");

        Self {
            pool,
            auth: Arc::new(auth),
        }
    }
}
