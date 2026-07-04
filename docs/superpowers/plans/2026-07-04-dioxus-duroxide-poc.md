# Dioxus + duroxide Order-Approval PoC Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Scaffold a full-stack Dioxus app whose `#[server]` functions start, query, and steer a human-in-the-loop duroxide "order approval" workflow persisted in PostgreSQL via `duroxide-pg`.

**Architecture:** A single Dioxus 0.7 fullstack crate. On server startup the process builds a `PostgresProvider`, registers duroxide activities + the `OrderApproval` orchestration, starts the duroxide `Runtime` (embedded), and stashes a `Client` in a process-global `OnceLock`. `#[server]` functions read that global to start orchestrations, query status, and raise the `approval` external event. The WASM client renders a form + polling table with Approve/Reject buttons that drive the workflow live.

**Tech Stack:** Rust, Dioxus 0.7 (`fullstack`), Axum 0.8, Tokio 1, duroxide 0.1 + duroxide-pg 0.1, serde/serde_json, uuid, dotenvy, Postgres 16 (docker-compose).

## Global Constraints

- **Dioxus** version `0.7` (fullstack). Client feature `web`, server feature `server`.
- **duroxide** `0.1`, **duroxide-pg** `0.1` (PostgresProvider, `v0.1.34`). Experimental crates — exact symbol paths/signatures may drift; Task 1 pins them against the installed source and every later task's code is adjusted to match those confirmed names.
- **Payloads are `String`** (JSON via `serde_json`) across the duroxide boundary — activity/orchestration inputs and outputs are strings, per the Durable-Task-Framework convention.
- **Postgres** reached via `DATABASE_URL` (e.g. `postgres://duroxide:duroxide@localhost:5432/duroxide`), loaded from `.env` with `dotenvy` on the server only.
- Server-only code (Axum, Tokio, duroxide, PostgresProvider, dotenvy) MUST be gated behind `#[cfg(feature = "server")]` — it does not compile to WASM.
- `.env` is git-ignored; commit `.env.example`.

---

## File Structure

- `Cargo.toml` — deps + `web`/`server` feature split.
- `Dioxus.toml` — app name/config for `dx serve`.
- `docker-compose.yml` — `postgres:16` service.
- `.env.example` / `.env` — `DATABASE_URL`.
- `.gitignore` — `/target`, `.env`, `/dist`.
- `README.md` — setup + run + demo steps.
- `docs/API-NOTES.md` — confirmed duroxide symbol paths/signatures (produced by Task 1).
- `src/main.rs` — `launch(App)` for client; `#[cfg(feature="server")]` custom Axum main that bootstraps duroxide then serves Dioxus.
- `src/workflow.rs` — (server-only) activities, `OrderApproval` orchestration, registries, `Runtime` bootstrap, `CLIENT`/`ORDERS` globals, DTO mapping helpers. Includes unit tests using duroxide's in-memory/SQLite provider.
- `src/server.rs` — `#[server]` functions + shared `OrderStatusDto`/`OrderInput` types.
- `src/app.rs` — UI components (form, table, decision buttons, polling).

---

## Task 1: Project scaffold, dependencies, Postgres, and API pinning

**Files:**
- Create: `Cargo.toml`, `Dioxus.toml`, `docker-compose.yml`, `.env.example`, `.env`, `.gitignore`, `src/main.rs`, `docs/API-NOTES.md`

**Interfaces:**
- Produces: a compiling skeleton (`App` component renders a placeholder) and `docs/API-NOTES.md` recording confirmed duroxide symbols consumed by Tasks 2–3.

- [ ] **Step 1: Create `Cargo.toml`**

```toml
[package]
name = "duroxus"
version = "0.1.0"
edition = "2021"

[dependencies]
dioxus = { version = "0.7", features = ["fullstack"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"

# Server-only (not WASM-compatible)
axum = { version = "0.8", optional = true }
tokio = { version = "1", features = ["full"], optional = true }
duroxide = { version = "0.1", optional = true }
duroxide-pg = { version = "0.1", optional = true }
uuid = { version = "1", features = ["v4"], optional = true }
dotenvy = { version = "0.15", optional = true }

[features]
default = ["web"]
web = ["dioxus/web"]
server = ["dioxus/server", "dep:axum", "dep:tokio", "dep:duroxide", "dep:duroxide-pg", "dep:uuid", "dep:dotenvy"]

[profile.dev]
opt-level = 1
```

- [ ] **Step 2: Create `Dioxus.toml`**

```toml
[application]
name = "duroxus"

[web.app]
title = "Duroxus — Order Approval"
```

- [ ] **Step 3: Create `docker-compose.yml`**

```yaml
services:
  postgres:
    image: postgres:16
    environment:
      POSTGRES_USER: duroxide
      POSTGRES_PASSWORD: duroxide
      POSTGRES_DB: duroxide
    ports:
      - "5432:5432"
    volumes:
      - pgdata:/var/lib/postgresql/data
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U duroxide"]
      interval: 3s
      timeout: 3s
      retries: 10
volumes:
  pgdata:
```

- [ ] **Step 4: Create `.env.example` and copy to `.env`**

`.env.example`:
```
DATABASE_URL=postgres://duroxide:duroxide@localhost:5432/duroxide
```
Run: `cp .env.example .env`

- [ ] **Step 5: Ensure `.gitignore` covers build + secrets**

```
/target
/dist
.env
```

- [ ] **Step 6: Create minimal `src/main.rs` (placeholder App, both targets compile)**

```rust
#![allow(non_snake_case)]
use dioxus::prelude::*;

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    rsx! {
        h1 { "Duroxus — Order Approval PoC" }
        p { "Scaffold online." }
    }
}
```

- [ ] **Step 7: Verify the client build compiles**

Run: `cargo check --features web --no-default-features`
Expected: PASS (only dioxus + serde compiled).

- [ ] **Step 8: Add the server deps and pin the duroxide API**

Run: `cargo check --features server --no-default-features`
Expected: PASS (all server crates resolve and download).

Then confirm the real symbol paths/signatures from the downloaded source (do NOT trust this plan's guesses over the source):

Run: `cargo doc -p duroxide -p duroxide-pg --no-deps` then open `target/doc/duroxide/index.html`, OR inspect source directly:
Run: `find ~/.cargo/registry/src -maxdepth 1 -type d -name 'duroxide-*'`
Run: `grep -rn "pub async fn start_orchestration\|pub async fn raise_event\|pub async fn get_status\|pub async fn wait_for_orchestration\|pub fn new" ~/.cargo/registry/src/*/duroxide-0*/src/client.rs 2>/dev/null | head -40`
Run: `grep -rn "pub async fn schedule_activity\|pub async fn schedule_wait\|pub async fn schedule_timer\|pub async fn select2\|enum Either2" ~/.cargo/registry/src/*/duroxide-0*/src/ 2>/dev/null | head -40`
Run: `grep -rn "start_with_store\|impl Runtime\|pub enum OrchestrationStatus" ~/.cargo/registry/src/*/duroxide-0*/src/ 2>/dev/null | head -40`
Run: `grep -rn "pub async fn new\|new_with_schema\|impl.*Provider" ~/.cargo/registry/src/*/duroxide-pg-0*/src/ 2>/dev/null | head -20`

- [ ] **Step 9: Record confirmed API in `docs/API-NOTES.md`**

Write the ACTUAL signatures found (fill in from Step 8 output — this is the source of truth for Tasks 2–3):

```markdown
# Confirmed duroxide API (installed versions)

duroxide: <version>   duroxide-pg: <version>

## Client
- constructor: Client::new(<arg type>) -> <ret>
- start_orchestration(<sig>) -> <ret>
- raise_event(<sig>) -> <ret>
- get_status(<sig>) -> <ret>            # note the OrchestrationStatus type/variants
- wait_for_orchestration(<sig>) -> <ret>

## OrchestrationContext
- schedule_activity(<sig>) -> <ret>
- schedule_wait(<sig>) -> <ret>
- schedule_timer(<sig>) -> <ret>
- select2(<sig>) -> <Either2 path + variants>

## Runtime
- start_with_store(<sig>) -> <ret>

## Registries
- ActivityRegistry::builder().register(name, fn).build()
- OrchestrationRegistry::builder().register(name, fn).build()

## PostgresProvider (duroxide-pg)
- new(<sig>) -> <ret>;  new_with_schema(<sig>) -> <ret>

## OrchestrationStatus variants
- <list variants + how to extract terminal output / error>
```

> **If any signature below in Tasks 2–3 disagrees with `API-NOTES.md`, `API-NOTES.md` wins.** Adjust the code to the confirmed API; the shapes here reflect docs.rs at planning time.

- [ ] **Step 10: Commit**

```bash
git add -A
git commit -m "chore: scaffold dioxus+duroxide crate, docker postgres, pin duroxide API"
```

---

## Task 2: Workflow module — activities, orchestration, runtime bootstrap, globals

**Files:**
- Create: `src/workflow.rs`
- Modify: `src/main.rs` (add `#[cfg(feature="server")] mod workflow;`)
- Test: inline `#[cfg(test)]` module in `src/workflow.rs`

**Interfaces:**
- Consumes: confirmed API from `docs/API-NOTES.md`.
- Produces (server-only, used by Task 3):
  - `pub const ORCHESTRATION_NAME: &str = "OrderApproval";`
  - `pub const APPROVAL_EVENT: &str = "approval";`
  - `pub async fn init(database_url: &str) -> Result<(), String>` — builds provider, starts runtime, sets `CLIENT`.
  - `pub fn client() -> Arc<duroxide::Client>` — panics if `init` not called.
  - `pub fn record_order(instance_id: &str)` / `pub fn all_orders() -> Vec<String>` — dashboard id registry.
  - `pub fn stage_from_status(status: &OrchestrationStatus) -> (String, bool)` — returns `(stage_label, actionable)`.

- [ ] **Step 1: Write the failing test (approve path + reject/compensation path)**

Add to `src/workflow.rs`. The test drives the orchestration through an **in-memory** provider (no Postgres) so it runs in CI. It starts the runtime, starts an instance, raises the `approval` event, and asserts the terminal output; a second case raises a reject and asserts the compensation result.

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    // Helper: build a runtime + client on duroxide's in-memory/sqlite store.
    // NOTE: adjust `in_memory_client()` to the confirmed SqliteProvider constructor
    // in docs/API-NOTES.md (e.g. SqliteProvider::new("sqlite::memory:", None)).
    async fn test_client() -> Arc<duroxide::Client> {
        let store = build_test_store().await;
        let (activities, orchestrations) = registries();
        let _rt = duroxide::runtime::Runtime::start_with_store(
            store.clone(), activities, orchestrations,
        ).await;
        // keep runtime alive for the test process
        std::mem::forget(_rt);
        Arc::new(duroxide::Client::new(store))
    }

    #[tokio::test]
    async fn approve_path_fulfills() {
        let client = test_client().await;
        let input = serde_json::json!({"item":"widget","amount":10}).to_string();
        client.start_orchestration("t-approve", ORCHESTRATION_NAME, input).await.unwrap();
        // let it reach the wait, then approve
        client.raise_event("t-approve", APPROVAL_EVENT, "approve".to_string()).await.unwrap();
        let out = client.wait_for_orchestration("t-approve", Duration::from_secs(10)).await.unwrap();
        assert!(format!("{out:?}").contains("FULFILLED"), "got {out:?}");
    }

    #[tokio::test]
    async fn reject_path_refunds() {
        let client = test_client().await;
        let input = serde_json::json!({"item":"widget","amount":10}).to_string();
        client.start_orchestration("t-reject", ORCHESTRATION_NAME, input).await.unwrap();
        client.raise_event("t-reject", APPROVAL_EVENT, "reject".to_string()).await.unwrap();
        let out = client.wait_for_orchestration("t-reject", Duration::from_secs(10)).await.unwrap();
        assert!(format!("{out:?}").contains("REFUNDED"), "got {out:?}");
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test --features server --no-default-features workflow`
Expected: FAIL (compile error — `registries`, `build_test_store`, `ORCHESTRATION_NAME`, etc. not defined).

- [ ] **Step 3: Implement `src/workflow.rs` (activities, orchestration, registries, globals, bootstrap)**

```rust
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use duroxide::{Client, OrchestrationContext, ActivityContext,
               ActivityRegistry, OrchestrationRegistry};
use duroxide::runtime::Runtime;

pub const ORCHESTRATION_NAME: &str = "OrderApproval";
pub const APPROVAL_EVENT: &str = "approval";
const APPROVAL_TIMEOUT_SECS: u64 = 120;

static CLIENT: OnceLock<Arc<Client>> = OnceLock::new();
static RUNTIME: OnceLock<Runtime> = OnceLock::new(); // keep runtime alive
static ORDERS: Mutex<Vec<String>> = Mutex::new(Vec::new());

pub fn client() -> Arc<Client> {
    CLIENT.get().expect("workflow::init not called").clone()
}

pub fn record_order(instance_id: &str) {
    ORDERS.lock().unwrap().push(instance_id.to_string());
}

pub fn all_orders() -> Vec<String> {
    ORDERS.lock().unwrap().clone()
}

// ---- Activities (stubs: log + short sleep + echo) ----
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

// ---- Orchestration: validate -> charge -> await approval(vs timeout) -> fulfill | refund ----
async fn order_approval(ctx: OrchestrationContext, input: String) -> Result<String, String> {
    ctx.schedule_activity("ValidateOrder", input.clone()).await?;
    ctx.schedule_activity("ChargePayment", input.clone()).await?;

    // Race a human decision against an auto-expiry timer.
    let approval = ctx.schedule_wait(APPROVAL_EVENT);
    let timeout = ctx.schedule_timer(Duration::from_secs(APPROVAL_TIMEOUT_SECS));

    // NOTE: confirm Either2 path + variant names in docs/API-NOTES.md.
    let decision = match ctx.select2(approval, timeout).await {
        duroxide::Either2::First(payload) => payload,        // "approve" / "reject"
        duroxide::Either2::Second(_) => "reject".to_string(), // timed out
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

// ---- Bootstrap (Postgres) ----
pub async fn init(database_url: &str) -> Result<(), String> {
    use duroxide_pg::PostgresProvider;
    let provider = PostgresProvider::new(database_url).await.map_err(|e| e.to_string())?;
    let store = Arc::new(provider);
    let (activities, orchestrations) = registries();
    let rt = Runtime::start_with_store(store.clone(), activities, orchestrations).await;
    let _ = RUNTIME.set(rt);
    let _ = CLIENT.set(Arc::new(Client::new(store)));
    Ok(())
}
```

- [ ] **Step 4: Add the test-store helper (in-memory/SQLite) inside the `tests` module or a `#[cfg(test)]` fn**

Add near the top of the `tests` module (adjust constructor to `API-NOTES.md`):

```rust
    async fn build_test_store() -> Arc<dyn duroxide::providers::Provider> {
        use duroxide::providers::sqlite::SqliteProvider;
        Arc::new(SqliteProvider::new("sqlite::memory:", None).await.unwrap())
    }
```

> If `Runtime::start_with_store` and `Client::new` want a concrete `Arc<SqliteProvider>` rather than `Arc<dyn Provider>` (as the docs.rs example suggests), change the return type to `Arc<SqliteProvider>`. Let the compiler decide; match `API-NOTES.md`.

- [ ] **Step 5: Register the module (server-only) in `src/main.rs`**

Add near the top of `src/main.rs`:
```rust
#[cfg(feature = "server")]
mod workflow;
```

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cargo test --features server --no-default-features workflow`
Expected: PASS — `approve_path_fulfills` and `reject_path_refunds` green.

> If they fail on symbol/signature mismatches, reconcile against `docs/API-NOTES.md` (the confirmed API), not against this plan. This is the expected place to absorb crate-version drift.

- [ ] **Step 7: Add `stage_from_status` mapping helper**

Append to `src/workflow.rs` (adjust `OrchestrationStatus` variants to the confirmed enum in `API-NOTES.md`):

```rust
use duroxide::OrchestrationStatus;

/// Maps a duroxide status to a UI stage label + whether Approve/Reject applies.
/// PoC simplification: because activities are fast stubs, a Running instance is
/// effectively parked at the approval wait, so Running == actionable "Awaiting approval".
pub fn stage_from_status(status: &OrchestrationStatus) -> (String, bool) {
    match status {
        OrchestrationStatus::Completed { output } => {
            let label = if output.contains("FULFILLED") { "Fulfilled" }
                        else if output.contains("REFUNDED") { "Refunded" }
                        else { "Completed" };
            (label.to_string(), false)
        }
        OrchestrationStatus::Failed { error } => (format!("Failed: {error}"), false),
        OrchestrationStatus::Running | OrchestrationStatus::Pending => {
            ("Awaiting approval".to_string(), true)
        }
        other => (format!("{other:?}"), false),
    }
}
```

- [ ] **Step 8: Run check to confirm the helper compiles**

Run: `cargo check --features server --no-default-features`
Expected: PASS.

- [ ] **Step 9: Commit**

```bash
git add -A
git commit -m "feat: order-approval orchestration, activities, runtime bootstrap + tests"
```

---

## Task 3: Server functions (Dioxus ↔ duroxide bridge)

**Files:**
- Create: `src/server.rs`
- Modify: `src/main.rs` (add `mod server;`)

**Interfaces:**
- Consumes: `workflow::{client, init, record_order, all_orders, stage_from_status, ORCHESTRATION_NAME, APPROVAL_EVENT}`.
- Produces (called from Task 4 UI):
  - `#[derive(Serialize, Deserialize, Clone, PartialEq)] pub struct OrderInput { pub item: String, pub amount: u32 }`
  - `#[derive(Serialize, Deserialize, Clone, PartialEq)] pub struct OrderStatusDto { pub instance_id: String, pub stage: String, pub actionable: bool }`
  - `async fn start_order(order: OrderInput) -> ServerFnResult<String>`
  - `async fn get_order_status(instance_id: String) -> ServerFnResult<OrderStatusDto>`
  - `async fn list_orders() -> ServerFnResult<Vec<OrderStatusDto>>`
  - `async fn submit_decision(instance_id: String, approve: bool) -> ServerFnResult<()>`

- [ ] **Step 1: Implement `src/server.rs`**

```rust
use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct OrderInput {
    pub item: String,
    pub amount: u32,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct OrderStatusDto {
    pub instance_id: String,
    pub stage: String,
    pub actionable: bool,
}

#[server]
pub async fn start_order(order: OrderInput) -> ServerFnResult<String> {
    use crate::workflow;
    let instance_id = format!("order-{}", uuid::Uuid::new_v4());
    let input = serde_json::to_string(&order).map_err(ServerFnError::new)?;
    workflow::client()
        .start_orchestration(&instance_id, workflow::ORCHESTRATION_NAME, input)
        .await
        .map_err(|e| ServerFnError::new(format!("{e:?}")))?;
    workflow::record_order(&instance_id);
    Ok(instance_id)
}

#[server]
pub async fn get_order_status(instance_id: String) -> ServerFnResult<OrderStatusDto> {
    use crate::workflow;
    let status = workflow::client()
        .get_status(&instance_id)
        .await
        .map_err(|e| ServerFnError::new(format!("{e:?}")))?;
    let (stage, actionable) = workflow::stage_from_status(&status);
    Ok(OrderStatusDto { instance_id, stage, actionable })
}

#[server]
pub async fn list_orders() -> ServerFnResult<Vec<OrderStatusDto>> {
    use crate::workflow;
    let mut out = Vec::new();
    for id in workflow::all_orders() {
        let status = workflow::client()
            .get_status(&id)
            .await
            .map_err(|e| ServerFnError::new(format!("{e:?}")))?;
        let (stage, actionable) = workflow::stage_from_status(&status);
        out.push(OrderStatusDto { instance_id: id, stage, actionable });
    }
    Ok(out)
}

#[server]
pub async fn submit_decision(instance_id: String, approve: bool) -> ServerFnResult<()> {
    use crate::workflow;
    let payload = if approve { "approve" } else { "reject" }.to_string();
    workflow::client()
        .raise_event(&instance_id, workflow::APPROVAL_EVENT, payload)
        .await
        .map_err(|e| ServerFnError::new(format!("{e:?}")))?;
    Ok(())
}
```

> `get_status` / `start_orchestration` / `raise_event` argument-by-value vs by-reference and exact `ServerFnError` constructor may differ — reconcile with `docs/API-NOTES.md` and the `dioxus::prelude` for `ServerFnError`/`ServerFnResult`. Keep the public function signatures above stable; Task 4 depends on them.

- [ ] **Step 2: Register the module in `src/main.rs`**

Add near the top of `src/main.rs`:
```rust
mod server;
```
(Not feature-gated: `#[server]` functions must exist on both client and server — the macro generates the client-side stub.)

- [ ] **Step 3: Verify both targets compile**

Run: `cargo check --features server --no-default-features`
Expected: PASS.
Run: `cargo check --features web --no-default-features`
Expected: PASS (server bodies compile out; client stubs remain).

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "feat: server functions bridging Dioxus to duroxide client"
```

---

## Task 4: UI — order form, polling status table, approve/reject

**Files:**
- Create: `src/app.rs`
- Modify: `src/main.rs` (replace placeholder `App` with `mod app; use app::App;`)

**Interfaces:**
- Consumes: `server::{OrderInput, OrderStatusDto, start_order, get_order_status, list_orders, submit_decision}`.
- Produces: `pub fn App() -> Element`.

- [ ] **Step 1: Implement `src/app.rs`**

```rust
#![allow(non_snake_case)]
use dioxus::prelude::*;
use crate::server::{
    OrderInput, OrderStatusDto, start_order, list_orders, submit_decision,
};

#[component]
pub fn App() -> Element {
    let mut item = use_signal(|| "widget".to_string());
    let mut amount = use_signal(|| "10".to_string());
    let mut orders = use_signal(Vec::<OrderStatusDto>::new);
    let mut error = use_signal(|| Option::<String>::None);

    // Poll the order list every 1500ms.
    use_future(move || async move {
        loop {
            match list_orders().await {
                Ok(list) => orders.set(list),
                Err(e) => error.set(Some(e.to_string())),
            }
            gloo_timers::future::TimeoutFuture::new(1500).await;
        }
    });

    let create = move |_| async move {
        let amt = amount().parse::<u32>().unwrap_or(0);
        match start_order(OrderInput { item: item(), amount: amt }).await {
            Ok(_id) => error.set(None),
            Err(e) => error.set(Some(e.to_string())),
        }
    };

    rsx! {
        h1 { "Duroxus — Order Approval" }
        if let Some(e) = error() {
            p { style: "color:red", "Error: {e}" }
        }
        div {
            input { value: "{item}", oninput: move |e| item.set(e.value()) }
            input { value: "{amount}", oninput: move |e| amount.set(e.value()) }
            button { onclick: create, "Create order" }
        }
        table {
            thead { tr { th { "Instance" } th { "Stage" } th { "Action" } } }
            tbody {
                for o in orders() {
                    OrderRow { order: o.clone() }
                }
            }
        }
    }
}

#[component]
fn OrderRow(order: OrderStatusDto) -> Element {
    let id = order.instance_id.clone();
    let decide = move |approve: bool| {
        let id = id.clone();
        async move { let _ = submit_decision(id, approve).await; }
    };
    let id_a = order.instance_id.clone();
    let id_r = order.instance_id.clone();
    rsx! {
        tr {
            td { "{order.instance_id}" }
            td { "{order.stage}" }
            td {
                if order.actionable {
                    button {
                        onclick: move |_| { let id = id_a.clone(); async move { let _ = submit_decision(id, true).await; } },
                        "Approve"
                    }
                    button {
                        onclick: move |_| { let id = id_r.clone(); async move { let _ = submit_decision(id, false).await; } },
                        "Reject"
                    }
                } else { "—" }
            }
        }
    }
}
```

> `use_future` + `gloo_timers` is the polling mechanism. If `gloo-timers` is not already transitively available, add `gloo-timers = { version = "0.3", features = ["futures"] }` under `[target.'cfg(target_arch = "wasm32")'.dependencies]`. Alternatively use Dioxus's own async sleep if the confirmed 0.7 API exposes one. Keep the visible behavior (poll ~every 1.5s) identical.

- [ ] **Step 2: Wire `App` into `src/main.rs`**

Replace the placeholder `App` with:
```rust
mod app;
use app::App;
```
(Remove the old `#[component] fn App` placeholder.)

- [ ] **Step 3: Verify both targets compile**

Run: `cargo check --features web --no-default-features`
Expected: PASS.
Run: `cargo check --features server --no-default-features`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "feat: UI with order form, polling status table, approve/reject"
```

---

## Task 5: Server entrypoint wiring, README, end-to-end verification

**Files:**
- Modify: `src/main.rs` (custom server main that loads `.env`, calls `workflow::init`, serves Dioxus)
- Create: `README.md`

**Interfaces:**
- Consumes: `workflow::init`, `server` functions (auto-registered by Dioxus).

- [ ] **Step 1: Replace `src/main.rs` with the dual-target entrypoint**

```rust
#![allow(non_snake_case)]
use dioxus::prelude::*;

mod app;
mod server;
use app::App;

#[cfg(feature = "server")]
mod workflow;

#[cfg(not(feature = "server"))]
fn main() {
    dioxus::launch(App);
}

#[cfg(feature = "server")]
#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    let database_url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL must be set (see .env)");

    workflow::init(&database_url).await.expect("failed to init duroxide runtime");

    use dioxus::server::{DioxusRouterExt, ServeConfig};
    let address = dioxus::cli_config::fullstack_address_or_localhost();
    let router = axum::Router::new()
        .serve_dioxus_application(ServeConfig::new().unwrap(), App)
        .into_make_service();
    let listener = tokio::net::TcpListener::bind(address).await.unwrap();
    println!("listening on http://{address}");
    axum::serve(listener, router).await.unwrap();
}
```

> The exact `serve_dioxus_application` / `ServeConfig` import path and whether `ServeConfig::new()` returns a `Result` changed across 0.7 releases. Reconcile the four server-main lines against the installed `dioxus` fullstack docs (`cargo doc -p dioxus`); keep the sequence identical: load `.env` → `workflow::init` → build router serving `App` → `axum::serve`.

- [ ] **Step 2: Create `README.md`**

````markdown
# Duroxus — Dioxus + duroxide Order-Approval PoC

A full-stack [Dioxus](https://dioxuslabs.com) app whose server functions drive a
human-in-the-loop [duroxide](https://github.com/microsoft/duroxide) durable workflow,
persisted in PostgreSQL via [duroxide-pg](https://github.com/microsoft/duroxide-pg).

## Prerequisites
- Rust (stable), `dx` CLI: `cargo install dioxus-cli`
- Docker (for Postgres)

## Run
```bash
cp .env.example .env
docker compose up -d          # Postgres 16 on :5432
dx serve                      # builds client + server, embeds the duroxide runtime
```
Open the printed URL.

## Demo
1. Enter an item + amount, click **Create order** → a durable `OrderApproval`
   orchestration starts (validate → charge → *awaiting approval*).
2. The table polls status every ~1.5s. A row in **Awaiting approval** shows
   **Approve** / **Reject**.
3. Click **Approve** → workflow runs `FulfillOrder`, row → **Fulfilled**.
   Click **Reject** (or wait for the 120s timer) → saga compensation `RefundPayment`,
   row → **Refunded**.
4. Durability check: `docker compose restart` mid-workflow — the orchestration state
   survives in Postgres and resumes. (The dashboard's in-memory id list resets on app
   restart; the workflows themselves are durable.)

## Notes
- Runtime topology: the duroxide runtime is **embedded** in the Dioxus server process.
- PoC simplification: because activities are fast stubs, a `Running` instance is treated
  as "Awaiting approval" in the UI.
- Tests: `cargo test --features server --no-default-features` runs the workflow
  approve/reject paths against an in-memory store (no Postgres needed).
````

- [ ] **Step 3: Build the whole app**

Run: `dx build` (or `cargo check --features server --no-default-features && cargo check --features web --no-default-features`)
Expected: PASS both targets.

- [ ] **Step 4: End-to-end verification against Postgres**

Run: `docker compose up -d`
Run: `dx serve` (leave running)
Then in a browser at the printed URL:
- Create an order → confirm a row appears and reaches **Awaiting approval**.
- Click **Approve** → row transitions to **Fulfilled** within ~2 polls.
- Create another → **Reject** → row transitions to **Refunded**.
Expected: all three transitions observed. Capture/note the outcome.

> Verification is manual (browser-driven) per the spec — the durable PG path is exercised
> at runtime, not in CI. If any transition stalls, inspect `dx serve` logs for the mapped
> `ServerFnError` and reconcile the failing call against `docs/API-NOTES.md`.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat: server entrypoint wiring, README, e2e demo steps"
```

---

## Self-Review

**Spec coverage:**
- Architecture/topology (single crate, embedded runtime, provide via global) → Tasks 1, 2, 5. ✓ (Design said `provide_context`; plan uses a process-global `OnceLock` for the shared `Client` — a deliberate robustness choice against Dioxus context-API churn, documented in Task 2 & the plan architecture note.)
- Workflow (validate→charge→wait/timer→fulfill|refund saga) → Task 2. ✓
- Server functions (start/status/decision/list) → Task 3. ✓
- UI (form, polling table, approve/reject) → Task 4. ✓
- docker-compose Postgres + `.env` → Task 1. ✓
- Error handling (ServerFnResult, per-row error) → Tasks 3, 4. ✓
- Testing (in-memory approve/reject unit tests + manual e2e) → Tasks 2, 5. ✓
- Layout (main/app/server/workflow) → matches spec. ✓

**Deviations from spec (intentional, noted):**
1. Shared `Client` via process-global `OnceLock` instead of `provide_context` — reduces exposure to Dioxus context-API drift; same effect (server fns reach the client).
2. Dashboard order list kept in an in-memory `Mutex<Vec<String>>` instead of querying an uncertain duroxide "list instances" API — documented as resetting on restart; workflows remain durable in PG.

**Placeholder scan:** No TBD/TODO left; every code step has complete code. The `> NOTE` blocks are API-reconciliation instructions (expected for experimental crates), not missing content.

**Type consistency:** `OrderInput`/`OrderStatusDto` defined in Task 3, consumed unchanged in Task 4. `workflow::{init, client, record_order, all_orders, stage_from_status, ORCHESTRATION_NAME, APPROVAL_EVENT}` defined in Task 2, consumed unchanged in Task 3. Server fn signatures declared in Task 3 interfaces match their call sites in Task 4. ✓
