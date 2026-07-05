//! Server-only duroxide workflow: the `OrderApproval` orchestration, its
//! activities, the embedded runtime bootstrap, and the process-global `Client`.
//!
//! Signatures here are pinned against duroxide 0.1.29 / duroxide-pg 0.1.34 —
//! see `docs/API-NOTES.md`.

use std::sync::Arc;
use std::time::Duration;

use duroxide::providers::Provider;
use duroxide::runtime::registry::ActivityRegistry;
use duroxide::runtime::Runtime;
use duroxide::{
    ActivityContext, Client, Either2, OrchestrationContext, OrchestrationRegistry,
    OrchestrationStatus,
};
use duroxide_pg::PostgresProvider;
use sqlx::PgPool;

pub const ORCHESTRATION_NAME: &str = "OrderApproval";
pub const APPROVAL_EVENT: &str = "approval";
const APPROVAL_TIMEOUT_SECS: u64 = 120;

// ---- Activities: stubs (log + echo), registered as closures so any future
//      state they need (e.g. a PgPool) is captured directly rather than
//      reached for through a static. Kept trivial so the PoC is about
//      orchestration. ----
fn registries(pool: PgPool) -> (ActivityRegistry, OrchestrationRegistry) {
    let activities = ActivityRegistry::builder()
        .register("ValidateOrder", {
            let pool = pool.clone();
            move |ctx, input| validate_order(pool.clone(), ctx, input)
        })
        .register("ChargePayment", {
            let pool = pool.clone();
            move |ctx, input| charge_payment(pool.clone(), ctx, input)
        })
        .register("FulfillOrder", {
            let pool = pool.clone();
            move |ctx, input| fulfill_order(pool.clone(), ctx, input)
        })
        .register("RefundPayment", {
            let pool = pool.clone();
            move |ctx, input| refund_payment(pool.clone(), ctx, input)
        })
        .build();

    let orchestrations = OrchestrationRegistry::builder()
        .register(ORCHESTRATION_NAME, order_approval)
        .build();

    (activities, orchestrations)
}

async fn validate_order(
    _pool: PgPool,
    _ctx: ActivityContext,
    input: String,
) -> Result<String, String> {
    Ok(format!("validated:{input}"))
}

async fn charge_payment(
    _pool: PgPool,
    _ctx: ActivityContext,
    input: String,
) -> Result<String, String> {
    Ok(format!("charged:{input}"))
}

async fn fulfill_order(
    _pool: PgPool,
    _ctx: ActivityContext,
    input: String,
) -> Result<String, String> {
    Ok(format!("fulfilled:{input}"))
}

async fn refund_payment(
    _pool: PgPool,
    _ctx: ActivityContext,
    input: String,
) -> Result<String, String> {
    Ok(format!("refunded:{input}"))
}

// ---- Orchestration: validate -> charge -> await approval (vs auto-expiry timer)
//      -> fulfill | refund (saga compensation). ----
async fn order_approval(ctx: OrchestrationContext, input: String) -> Result<String, String> {
    ctx.schedule_activity("ValidateOrder", input.clone())
        .await?;
    ctx.schedule_activity("ChargePayment", input.clone())
        .await?;

    // Race a human decision against an auto-expiry timer.
    let approval = ctx.schedule_wait(APPROVAL_EVENT);
    let timeout = ctx.schedule_timer(Duration::from_secs(APPROVAL_TIMEOUT_SECS));

    let decision = match ctx.select2(approval, timeout).await {
        Either2::First(payload) => payload, // "approve" / "reject"
        Either2::Second(()) => "reject".to_string(), // timed out -> reject
    };

    if decision.trim().eq_ignore_ascii_case("approve") {
        ctx.schedule_activity("FulfillOrder", input).await?;
        Ok("FULFILLED".to_string())
    } else {
        // saga compensation: refund the earlier charge
        ctx.schedule_activity("RefundPayment", input).await?;
        Ok("REFUNDED".to_string())
    }
}

/// Maps a duroxide status to a UI stage label + whether Approve/Reject applies.
///
/// PoC simplification: activities are fast stubs, so a `Running` instance is
/// effectively parked at the approval wait — hence Running == actionable
/// "Awaiting approval".
pub fn stage_from_status(status: &OrchestrationStatus) -> (String, bool) {
    match status {
        OrchestrationStatus::Completed { output, .. } => {
            let label = if output.contains("FULFILLED") {
                "Fulfilled"
            } else if output.contains("REFUNDED") {
                "Refunded"
            } else {
                "Completed"
            };
            (label.to_string(), false)
        }
        OrchestrationStatus::Failed { details, .. } => {
            (format!("Failed: {}", details.display_message()), false)
        }
        OrchestrationStatus::Running { .. } => ("Awaiting approval".to_string(), true),
        OrchestrationStatus::NotFound => ("Unknown".to_string(), false),
    }
}

/// Bootstrap the workflow runtime and control-plane client. The caller owns
/// both handles for the life of the process (e.g. by holding them in `main`)
/// instead of stashing them in statics.
pub async fn init(provider: PostgresProvider) -> (Arc<Runtime>, Arc<Client>) {
    let (activities, orchestrations) = registries(provider.pool().clone());
    let store: Arc<dyn Provider> = Arc::new(provider);
    let runtime = Runtime::start_with_store(store.clone(), activities, orchestrations).await;
    let client = Arc::new(Client::new(store));
    (runtime, client)
}

#[cfg(test)]
mod tests {
    use crate::state::AppState;

    use super::*;

    // Full order lifecycle against real Postgres (requires DATABASE_URL + docker).
    //
    // This is a SINGLE test on purpose: each `#[tokio::test]` gets its own tokio
    // runtime, and both the sqlx pool and duroxide's spawned dispatchers are bound
    // to the runtime that created them — so they cannot be shared across separate
    // test functions. duroxide also expects ONE runtime per store/schema; a second
    // runtime on the same `public` queues contends and deadlocks. So we run one
    // runtime and drive every case here. Isolated by unique instance ids.
    async fn drive(
        client: &Client,
        pool: &sqlx::PgPool,
        item: &str,
        amount: u32,
        decision: &str,
    ) -> (OrchestrationStatus, crate::orders::OrderRow) {
        let instance = format!("wf-{decision}-{}", uuid::Uuid::new_v4());
        let input = serde_json::json!({ "item": item, "amount": amount }).to_string();
        client
            .start_orchestration(&instance, ORCHESTRATION_NAME, input)
            .await
            .unwrap();
        crate::orders::insert(pool, &instance, item, amount)
            .await
            .unwrap();

        tokio::time::sleep(Duration::from_millis(700)).await;
        client
            .raise_event(&instance, APPROVAL_EVENT, decision)
            .await
            .unwrap();
        let status = client
            .wait_for_orchestration(&instance, Duration::from_secs(15))
            .await
            .unwrap();
        let row = crate::orders::get(pool, &instance).await.unwrap();
        (status, row)
    }

    #[tokio::test]
    async fn postgres_order_lifecycle() {
        let state = AppState::new().await;

        // 1) orders store CRUD sanity (same shared pool).
        let probe = format!("probe-{}", uuid::Uuid::new_v4());
        crate::orders::insert(&state.pool, &probe, "Probe", 7)
            .await
            .unwrap();
        let got = crate::orders::get(&state.pool, &probe).await.unwrap();
        assert_eq!(got.item, "Probe");
        assert_eq!(got.amount, 7);
        assert!(crate::orders::list(&state.pool)
            .await
            .unwrap()
            .iter()
            .any(|o| o.instance_id == probe));

        // 2) approve path -> persisted order + FULFILLED.
        let (status, row) = drive(&state.client, &state.pool, "Widget", 10, "approve").await;
        assert_eq!(row.item, "Widget");
        assert_eq!(row.amount, 10);
        assert!(
            matches!(&status, OrchestrationStatus::Completed { output, .. } if output.contains("FULFILLED")),
            "approve got {status:?}"
        );

        // 3) reject path -> persisted order + REFUNDED (saga compensation).
        let (status, row) = drive(&state.client, &state.pool, "Gadget", 42, "reject").await;
        assert_eq!(row.item, "Gadget");
        assert_eq!(row.amount, 42);
        assert!(
            matches!(&status, OrchestrationStatus::Completed { output, .. } if output.contains("REFUNDED")),
            "reject got {status:?}"
        );

        state._runtime.shutdown(Some(2000)).await;
    }
}
