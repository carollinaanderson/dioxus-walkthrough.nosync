//! graphile_worker task handlers for the order pipeline:
//! ValidateOrder -> ChargePayment -> FulfillOrder.
//!
//! Each handler stamps its stage onto `orders.status`, simulates work with a
//! short sleep (so the polling UI visibly walks the stages), then enqueues the
//! next job. Any error marks the order `failed` before surfacing the error to
//! graphile_worker.

use std::time::Duration;

use graphile_worker::{
    IntoTaskHandlerResult, JobSpec, TaskHandler, WorkerContext, WorkerContextExt,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

const STEP_DELAY: Duration = Duration::from_millis(1200);

async fn set_stage(ctx: &WorkerContext, order_id: Uuid, stage: &str) -> Result<(), String> {
    crate::orders::set_status(ctx.pg_pool(), order_id, stage)
        .await
        .map_err(|e| e.to_string())
}

async fn enqueue<T: TaskHandler + 'static>(ctx: &WorkerContext, job: T) -> Result<(), String> {
    ctx.add_job(job, JobSpec::default())
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Mark the order failed if a step errored, passing the error through.
async fn or_fail(
    ctx: &WorkerContext,
    order_id: Uuid,
    res: Result<(), String>,
) -> Result<(), String> {
    if res.is_err() {
        let _ = crate::orders::set_status(ctx.pg_pool(), order_id, "failed").await;
    }
    res
}

#[derive(Serialize, Deserialize)]
pub struct ValidateOrder {
    pub order_id: Uuid,
}

impl TaskHandler for ValidateOrder {
    const IDENTIFIER: &'static str = "validate_order";

    async fn run(self, ctx: WorkerContext) -> impl IntoTaskHandlerResult {
        let step = async {
            set_stage(&ctx, self.order_id, "validating").await?;
            tokio::time::sleep(STEP_DELAY).await;
            enqueue(
                &ctx,
                ChargePayment {
                    order_id: self.order_id,
                },
            )
            .await
        }
        .await;
        or_fail(&ctx, self.order_id, step).await
    }
}

#[derive(Serialize, Deserialize)]
pub struct ChargePayment {
    pub order_id: Uuid,
}

impl TaskHandler for ChargePayment {
    const IDENTIFIER: &'static str = "charge_payment";

    async fn run(self, ctx: WorkerContext) -> impl IntoTaskHandlerResult {
        let step = async {
            set_stage(&ctx, self.order_id, "charging").await?;
            tokio::time::sleep(STEP_DELAY).await;
            enqueue(
                &ctx,
                FulfillOrder {
                    order_id: self.order_id,
                },
            )
            .await
        }
        .await;
        or_fail(&ctx, self.order_id, step).await
    }
}

#[derive(Serialize, Deserialize)]
pub struct FulfillOrder {
    pub order_id: Uuid,
}

impl TaskHandler for FulfillOrder {
    const IDENTIFIER: &'static str = "fulfill_order";

    async fn run(self, ctx: WorkerContext) -> impl IntoTaskHandlerResult {
        let step = async {
            set_stage(&ctx, self.order_id, "fulfilling").await?;
            tokio::time::sleep(STEP_DELAY).await;
            set_stage(&ctx, self.order_id, "fulfilled").await
        }
        .await;
        or_fail(&ctx, self.order_id, step).await
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::state::AppState;

    /// End-to-end against real Postgres (docker compose up -d): a queued order
    /// walks validating -> charging -> fulfilling -> fulfilled once the worker
    /// picks up the chained jobs.
    #[tokio::test]
    async fn order_pipeline_runs_to_fulfilled() {
        let (state, worker) = AppState::new().await;
        let worker_handle = tokio::spawn(async move { worker.run().await });

        let hash = crate::auth::hash_password("hunter2-integration").unwrap();
        let username = format!("it-user-{}", uuid::Uuid::new_v4());
        let user = crate::users::insert(&state.pool, &username, &hash)
            .await
            .unwrap();

        let row = crate::orders::insert(&state.pool, user.id, "Widget", 10)
            .await
            .unwrap();
        assert_eq!(row.status, "queued");

        state
            .utils
            .add_job(ValidateOrder { order_id: row.id }, JobSpec::default())
            .await
            .unwrap();

        let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
        let mut seen = vec![row.status.clone()];
        loop {
            tokio::time::sleep(Duration::from_millis(200)).await;
            let cur = crate::orders::get_for_user(&state.pool, user.id, row.id)
                .await
                .unwrap()
                .expect("order row should exist");
            if seen.last() != Some(&cur.status) {
                seen.push(cur.status.clone());
            }
            if cur.status == "fulfilled" {
                break;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "pipeline timed out; stages seen: {seen:?}"
            );
        }
        // The pipeline passed through at least one intermediate stage.
        assert!(
            seen.contains(&"validating".to_string()),
            "stages seen: {seen:?}"
        );

        // list_for_user surfaces the fulfilled order (and only this user's).
        let listed = crate::orders::list_for_user(&state.pool, user.id)
            .await
            .unwrap();
        assert!(listed
            .iter()
            .any(|o| o.id == row.id && o.status == "fulfilled"));

        worker_handle.abort();
    }
}
