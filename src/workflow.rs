//! Server-only duroxide workflow: the `OrderApproval` orchestration, its
//! activities, the embedded runtime bootstrap, and the process-global `Client`.
//!
//! Signatures here are pinned against duroxide 0.1.29 / duroxide-pg 0.1.34 —
//! see `docs/API-NOTES.md`.

use std::sync::{Arc, Mutex, OnceLock};
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
static ORDERS: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// The shared duroxide control-plane client. Panics if `init` has not run.
pub fn client() -> Arc<Client> {
    CLIENT.get().expect("workflow::init not called").clone()
}

/// Remember an instance id so the dashboard can list it.
pub fn record_order(instance_id: &str) {
    ORDERS.lock().unwrap().push(instance_id.to_string());
}

/// All instance ids started this process (in-memory; resets on restart).
pub fn all_orders() -> Vec<String> {
    ORDERS.lock().unwrap().clone()
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
    use duroxide::providers::sqlite::SqliteProvider;

    // Build an in-memory-SQLite-backed client + running runtime for tests.
    async fn test_client() -> Arc<Client> {
        let store: Arc<dyn Provider> =
            Arc::new(SqliteProvider::new_in_memory().await.unwrap());
        let (activities, orchestrations) = registries();
        let rt = Runtime::start_with_store(store.clone(), activities, orchestrations).await;
        std::mem::forget(rt); // keep runtime workers alive for the test process
        Arc::new(Client::new(store))
    }

    #[tokio::test]
    async fn approve_path_fulfills() {
        let client = test_client().await;
        let input = serde_json::json!({"item":"widget","amount":10}).to_string();
        client
            .start_orchestration("t-approve", ORCHESTRATION_NAME, input)
            .await
            .unwrap();
        // let it reach the approval wait, then approve
        tokio::time::sleep(Duration::from_millis(500)).await;
        client
            .raise_event("t-approve", APPROVAL_EVENT, "approve")
            .await
            .unwrap();
        let out = client
            .wait_for_orchestration("t-approve", Duration::from_secs(10))
            .await
            .unwrap();
        assert!(
            matches!(&out, OrchestrationStatus::Completed { output, .. } if output.contains("FULFILLED")),
            "got {out:?}"
        );
    }

    #[tokio::test]
    async fn reject_path_refunds() {
        let client = test_client().await;
        let input = serde_json::json!({"item":"widget","amount":10}).to_string();
        client
            .start_orchestration("t-reject", ORCHESTRATION_NAME, input)
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(500)).await;
        client
            .raise_event("t-reject", APPROVAL_EVENT, "reject")
            .await
            .unwrap();
        let out = client
            .wait_for_orchestration("t-reject", Duration::from_secs(10))
            .await
            .unwrap();
        assert!(
            matches!(&out, OrchestrationStatus::Completed { output, .. } if output.contains("REFUNDED")),
            "got {out:?}"
        );
    }
}
