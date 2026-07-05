//! Server-only shared state, threaded into `#[server]` functions via
//! `axum::Extension` instead of process-global statics.

use std::sync::Arc;

use duroxide::{runtime::Runtime, Client};
use sqlx::PgPool;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub client: Arc<Client>,
    pub _runtime: Arc<Runtime>,
}

impl AppState {
    pub async fn new() -> Self {
        dotenvy::dotenv().ok();
        let database_url =
            std::env::var("DATABASE_URL").expect("DATABASE_URL must be set (see .env)");

        let provider = duroxide_pg::PostgresProvider::new(&database_url)
            .await
            .expect("failed to initialize postgres provider");
        let pool = provider.pool().clone();
        crate::orders::init(&pool)
            .await
            .expect("failed to initialize orders");
        let (_runtime, client) = crate::workflow::init(provider).await;
        Self {
            pool,
            client,
            _runtime,
        }
    }
}
