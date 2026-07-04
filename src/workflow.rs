//! Server-only duroxide workflow: the `OrderApproval` orchestration, its
//! activities, the embedded runtime bootstrap, and the process-global `Client`.
//!
//! Signatures here are pinned against duroxide 0.1.29 / duroxide-pg 0.1.34 —
//! see `docs/API-NOTES.md`.

use std::sync::{Arc, OnceLock};
use std::time::Duration;

use duroxide::providers::Provider;
use duroxide::runtime::registry::ActivityRegistry;
use duroxide::runtime::Runtime;
use duroxide::{
    ActivityContext, Client, Either2, OrchestrationContext, OrchestrationRegistry,
    OrchestrationStatus,
};

pub const ORCHESTRATION_NAME: &str = "OrderApproval";
pub const APPROVAL_EVENT: &str = "approval";
const APPROVAL_TIMEOUT_SECS: u64 = 120;

static CLIENT: OnceLock<Arc<Client>> = OnceLock::new();
static RUNTIME: OnceLock<Arc<Runtime>> = OnceLock::new(); // keep the runtime alive

/// The shared duroxide control-plane client. Panics if `init` has not run.
pub fn client() -> Arc<Client> {
    CLIENT.get().expect("workflow::init not called").clone()
}

// ---- Activities: stubs (log + echo). Kept trivial so the PoC is about orchestration. ----
fn registries() -> (ActivityRegistry, OrchestrationRegistry) {
    let activities = ActivityRegistry::builder()
        .register("ValidateOrder", |_ctx: ActivityContext, input: String| async move {
            Ok::<String, String>(format!("validated:{input}"))
        })
        .register("ChargePayment", |_ctx: ActivityContext, input: String| async move {
            Ok::<String, String>(format!("charged:{input}"))
        })
        .register("FulfillOrder", |_ctx: ActivityContext, input: String| async move {
            Ok::<String, String>(format!("fulfilled:{input}"))
        })
        .register("RefundPayment", |_ctx: ActivityContext, input: String| async move {
            Ok::<String, String>(format!("refunded:{input}"))
        })
        .build();

    let orchestrations = OrchestrationRegistry::builder()
        .register(ORCHESTRATION_NAME, order_approval)
        .build();

    (activities, orchestrations)
}

// ---- Orchestration: validate -> charge -> await approval (vs auto-expiry timer)
//      -> fulfill | refund (saga compensation). ----
async fn order_approval(ctx: OrchestrationContext, input: String) -> Result<String, String> {
    ctx.schedule_activity("ValidateOrder", input.clone()).await?;
    ctx.schedule_activity("ChargePayment", input.clone()).await?;

    // Race a human decision against an auto-expiry timer.
    let approval = ctx.schedule_wait(APPROVAL_EVENT);
    let timeout = ctx.schedule_timer(Duration::from_secs(APPROVAL_TIMEOUT_SECS));

    let decision = match ctx.select2(approval, timeout).await {
        Either2::First(payload) => payload,          // "approve" / "reject"
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

/// Bootstrap the Postgres-backed runtime and control-plane client.
pub async fn init(database_url: &str) -> Result<(), String> {
    use duroxide_pg::PostgresProvider;
    let provider = PostgresProvider::new(database_url)
        .await
        .map_err(|e| e.to_string())?;
    // Reuse duroxide-pg's pool for the orders table.
    crate::orders::init(provider.pool().clone()).await?;
    let store: Arc<dyn Provider> = Arc::new(provider);
    let (activities, orchestrations) = registries();
    let rt = Runtime::start_with_store(store.clone(), activities, orchestrations).await;
    let _ = RUNTIME.set(rt);
    let _ = CLIENT.set(Arc::new(Client::new(store)));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use duroxide_pg::PostgresProvider;

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
        item: &str,
        amount: u32,
        decision: &str,
    ) -> (OrchestrationStatus, crate::orders::OrderRow) {
        let instance = format!("wf-{decision}-{}", uuid::Uuid::new_v4());
        let input = serde_json::json!({ "item": item, "amount": amount }).to_string();
        client.start_orchestration(&instance, ORCHESTRATION_NAME, input).await.unwrap();
        crate::orders::insert(&instance, item, amount).await.unwrap();

        tokio::time::sleep(Duration::from_millis(700)).await;
        client.raise_event(&instance, APPROVAL_EVENT, decision).await.unwrap();
        let status = client
            .wait_for_orchestration(&instance, Duration::from_secs(15))
            .await
            .unwrap();
        let row = crate::orders::get(&instance).await.unwrap().expect("order row present");
        (status, row)
    }

    #[tokio::test]
    async fn postgres_order_lifecycle() {
        let url = std::env::var("DATABASE_URL").expect("DATABASE_URL (docker postgres) required");
        let provider = PostgresProvider::new(&url).await.unwrap();
        // orders reuses duroxide-pg's pool, exactly like the app.
        crate::orders::init(provider.pool().clone()).await.unwrap();

        let store: Arc<dyn Provider> = Arc::new(provider);
        let (activities, orchestrations) = registries();
        let rt = Runtime::start_with_store(store.clone(), activities, orchestrations).await;
        let client = Arc::new(Client::new(store));

        // 1) orders store CRUD sanity (same shared pool).
        let probe = format!("probe-{}", uuid::Uuid::new_v4());
        crate::orders::insert(&probe, "Probe", 7).await.unwrap();
        let got = crate::orders::get(&probe).await.unwrap().expect("probe row present");
        assert_eq!(got.item, "Probe");
        assert_eq!(got.amount, 7);
        assert!(crate::orders::list().await.unwrap().iter().any(|o| o.instance_id == probe));

        // 2) approve path -> persisted order + FULFILLED.
        let (status, row) = drive(&client, "Widget", 10, "approve").await;
        assert_eq!(row.item, "Widget");
        assert_eq!(row.amount, 10);
        assert!(
            matches!(&status, OrchestrationStatus::Completed { output, .. } if output.contains("FULFILLED")),
            "approve got {status:?}"
        );

        // 3) reject path -> persisted order + REFUNDED (saga compensation).
        let (status, row) = drive(&client, "Gadget", 42, "reject").await;
        assert_eq!(row.item, "Gadget");
        assert_eq!(row.amount, 42);
        assert!(
            matches!(&status, OrchestrationStatus::Completed { output, .. } if output.contains("REFUNDED")),
            "reject got {status:?}"
        );

        rt.shutdown(Some(2000)).await;
    }
}
