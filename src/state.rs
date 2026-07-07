//! Server-only shared state, threaded into `#[server]` functions via
//! `axum::Extension` instead of process-global statics.

use std::sync::Arc;

use graphile_worker::{Worker, WorkerOptions, WorkerUtils};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub utils: Arc<WorkerUtils>,
}

impl AppState {
    /// Connect Postgres, run app migrations, and initialize the
    /// graphile_worker worker (which creates/migrates its own
    /// `graphile_worker` schema). Returns the state plus the worker for the
    /// caller to `run()` — typically spawned as a background task.
    pub async fn new() -> (Self, Worker) {
        dotenvy::dotenv().ok();
        let database_url =
            std::env::var("DATABASE_URL").expect("DATABASE_URL must be set (see .env)");

        let pool = PgPoolOptions::new()
            .max_connections(10)
            .connect(&database_url)
            .await
            .expect("failed to connect to postgres");
        crate::orders::init(&pool)
            .await
            .expect("failed to run migrations");

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
        let utils = Arc::new(worker.create_utils());

        (Self { pool, utils }, worker)
    }
}
