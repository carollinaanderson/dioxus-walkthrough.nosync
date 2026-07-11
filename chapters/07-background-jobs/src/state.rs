//! Server-only shared state, threaded into `#[server]` functions via
//! `axum::Extension` instead of process-global statics.

use std::sync::Arc;

use better_auth::adapters::SqlxAdapter;
use better_auth::plugins::{EmailPasswordPlugin, SessionManagementPlugin};
use better_auth::{AuthBuilder, AuthConfig, BetterAuth};
use graphile_worker::runner::WorkerRuntimeError;
use graphile_worker::{WorkerOptions, WorkerUtils};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use tokio::task::JoinHandle;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub auth: Arc<BetterAuth<SqlxAdapter>>,
    pub worker: WorkerUtils,
}

impl AppState {
    /// Connect Postgres, run app migrations, build better-auth.rs, and
    /// initialize the graphile_worker worker (which creates/migrates its
    /// own `graphile_worker` schema). Returns the state plus the worker
    /// background task.
    pub async fn new() -> (Self, JoinHandle<Result<(), WorkerRuntimeError>>) {
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

        let worker = WorkerOptions::default()
            .pg_pool(pool.clone())
            .schema("graphile_worker")
            .concurrency(2)
            .define_job::<crate::jobs::ValidateOrder>()
            .define_job::<crate::jobs::ChargePayment>()
            .define_job::<crate::jobs::FulfillOrder>()
            .init()
            .await
            .expect("failed to initialize graphile_worker");
        let worker_utils = worker.create_utils();
        let worker_handle = tokio::spawn(async move { worker.run().await });
        (
            Self {
                pool,
                auth: Arc::new(auth),
                worker: worker_utils,
            },
            worker_handle,
        )
    }
}
