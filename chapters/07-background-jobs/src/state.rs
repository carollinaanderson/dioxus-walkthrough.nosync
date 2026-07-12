//! Server-only shared state, threaded into `#[server]` functions via
//! `axum::Extension` instead of process-global statics.

use graphile_worker::runner::WorkerRuntimeError;
use graphile_worker::{WorkerOptions, WorkerUtils};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use tokio::task::JoinHandle;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub worker: WorkerUtils,
}

impl AppState {
    /// Connect Postgres, run app migrations, and initialize the
    /// graphile_worker worker (which creates/migrates its own
    /// `graphile_worker` schema). Returns the state plus the worker
    /// background task. Accounts and sessions live in Clerk, not here.
    pub async fn new() -> (Self, JoinHandle<Result<(), WorkerRuntimeError>>) {
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
                worker: worker_utils,
            },
            worker_handle,
        )
    }
}
