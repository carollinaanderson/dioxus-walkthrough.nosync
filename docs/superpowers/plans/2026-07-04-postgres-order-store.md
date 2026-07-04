# Postgres-backed Order Store Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Persist order business-data (item, amount, created_at) in Postgres via sqlx — reusing duroxide-pg's connection pool and applying the schema through sqlx migrations — replacing the in-memory dashboard registry; move tests to real Postgres.

**Architecture:** A new server-only `src/orders.rs` owns an `orders` table and its queries, using a `PgPool` cloned from `PostgresProvider::pool()` (the same pool duroxide uses). The schema is a sqlx migration under `migrations/`, run at startup via `sqlx::migrate!(...).run(&pool)`. Server functions join durable order rows (item/amount) with live duroxide status at read time.

**Tech Stack:** Rust, Dioxus 0.7.9, duroxide 0.1.29 + duroxide-pg 0.1.34, **sqlx 0.8.6** (postgres, runtime-tokio, migrate), Postgres 16.

## Global Constraints

- Reuse duroxide-pg's pool: `provider.pool()` returns `&PgPool`; `PgPool` is Arc-backed, so `provider.pool().clone()` shares the same pool. **Do not** create a second pool in the app.
- sqlx pinned to `0.8` (matches duroxide-pg's 0.8.6 so the `PgPool` type is shared).
- Use sqlx's **runtime** query API (`sqlx::query`, `.bind`, `Row::get`) — NOT the compile-time `query!`/`query_as!` macros — so no live DB or `sqlx prepare` is needed at build time.
- Schema is applied ONLY via the sqlx migration (`migrations/0001_create_orders.sql`); no inline `CREATE TABLE` in Rust.
- `orders` lives in the pool's default schema (`public` for `PostgresProvider::new`). duroxide qualifies its own queries and does not set a persistent `search_path`, so there is no table-name conflict.
- `amount` is `u32` in Rust, stored as `BIGINT` (`i64`) in Postgres — bind as `i64`, read as `i64` then cast.
- Tests require Docker Postgres (`DATABASE_URL` set), isolate via **unique instance ids** (uuid) and assert only on their own ids, and run serially (`--test-threads=1`).
- Server-only code stays behind `#[cfg(feature = "server")]`.

---

## File Structure

- `migrations/0001_create_orders.sql` — the `orders` table DDL (sqlx migration).
- `src/orders.rs` — (server-only) `PgPool` global, sqlx migrator, `init`/`pool`/`insert`/`list`/`get`, `OrderRow`, plus a roundtrip PG test.
- `src/workflow.rs` — `init` reuses the pool and calls `orders::init`; drop `ORDERS`/`record_order`/`all_orders`; PG-backed approve/reject tests replacing the SQLite ones.
- `src/server.rs` — `OrderStatusDto` gains `item`/`amount`; `start_order` inserts; `list_orders`/`get_order_status` read `orders` + live status.
- `src/app.rs` — table gains Item/Amount columns.
- `src/main.rs` — add `#[cfg(feature="server")] mod orders;`.
- `Cargo.toml` — add sqlx; remove `test-support` feature + `duroxide/sqlite`.
- `README.md`, `docs/API-NOTES.md` — updated notes + test command.

---

## Task 1: orders module — migration, sqlx queries, roundtrip test

**Files:**
- Create: `migrations/0001_create_orders.sql`, `src/orders.rs`
- Modify: `Cargo.toml`, `src/main.rs`

**Interfaces:**
- Produces (server-only):
  - `pub struct OrderRow { pub instance_id: String, pub item: String, pub amount: u32 }`
  - `pub async fn init(pool: sqlx::PgPool) -> Result<(), String>` — run migrations + store pool.
  - `pub fn pool() -> sqlx::PgPool` — clone of the stored pool; panics if `init` not called.
  - `pub async fn insert(instance_id: &str, item: &str, amount: u32) -> Result<(), sqlx::Error>`
  - `pub async fn list() -> Result<Vec<OrderRow>, sqlx::Error>`
  - `pub async fn get(instance_id: &str) -> Result<Option<OrderRow>, sqlx::Error>`

- [ ] **Step 1: Add sqlx and drop the sqlite test feature in `Cargo.toml`**

Add to `[dependencies]` (server-only):
```toml
sqlx = { version = "0.8", features = ["runtime-tokio", "postgres", "migrate"], optional = true }
```
Update the `server` feature to include `dep:sqlx`, and remove the `test-support` line entirely:
```toml
server = ["dioxus/server", "dep:axum", "dep:tokio", "dep:duroxide", "dep:duroxide-pg", "dep:uuid", "dep:dotenvy", "dep:sqlx"]
```
(Delete the previous `test-support = ["server", "duroxide/sqlite"]` line.)

- [ ] **Step 2: Create the migration `migrations/0001_create_orders.sql`**

```sql
CREATE TABLE IF NOT EXISTS orders (
    instance_id TEXT PRIMARY KEY,
    item        TEXT        NOT NULL,
    amount      BIGINT      NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
```

- [ ] **Step 3: Register the module in `src/main.rs`**

Add alongside the other server-only module declaration:
```rust
#[cfg(feature = "server")]
mod orders;
```

- [ ] **Step 4: Write the failing roundtrip test in `src/orders.rs`**

Create `src/orders.rs` with ONLY the test module first (so it fails to compile → RED):
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn insert_get_list_roundtrip() {
        let url = std::env::var("DATABASE_URL").expect("DATABASE_URL (docker postgres) required");
        let pool = sqlx::PgPool::connect(&url).await.unwrap();
        init(pool.clone()).await.unwrap();

        let id = format!("test-roundtrip-{}", uuid::Uuid::new_v4());
        insert(&id, "Widget", 10).await.unwrap();

        let got = get(&id).await.unwrap().expect("row present");
        assert_eq!(got.item, "Widget");
        assert_eq!(got.amount, 10);

        let all = list().await.unwrap();
        assert!(all.iter().any(|o| o.instance_id == id && o.amount == 10 && o.item == "Widget"));
    }
}
```

- [ ] **Step 5: Run the test to verify it fails (compile error)**

Run: `cargo test --features server --no-default-features orders 2>&1 | tail -15`
Expected: FAIL — `init`, `insert`, `get`, `list`, `OrderRow` not found.

- [ ] **Step 6: Implement `src/orders.rs` (prepend above the test module)**

```rust
//! Server-only Postgres store for order business-data. Reuses duroxide-pg's
//! `PgPool` (see docs/API-NOTES.md). Schema is applied via the sqlx migration
//! in `migrations/`.

use std::sync::OnceLock;

use sqlx::{PgPool, Row};

static POOL: OnceLock<PgPool> = OnceLock::new();
static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

#[derive(Debug, Clone, PartialEq)]
pub struct OrderRow {
    pub instance_id: String,
    pub item: String,
    pub amount: u32,
}

/// Store the (shared) pool and apply pending sqlx migrations.
pub async fn init(pool: PgPool) -> Result<(), String> {
    MIGRATOR.run(&pool).await.map_err(|e| e.to_string())?;
    let _ = POOL.set(pool);
    Ok(())
}

/// Clone of the stored pool. Panics if `init` has not run.
pub fn pool() -> PgPool {
    POOL.get().expect("orders::init not called").clone()
}

pub async fn insert(instance_id: &str, item: &str, amount: u32) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO orders (instance_id, item, amount) VALUES ($1, $2, $3)")
        .bind(instance_id)
        .bind(item)
        .bind(amount as i64)
        .execute(&pool())
        .await?;
    Ok(())
}

fn row_from(r: &sqlx::postgres::PgRow) -> OrderRow {
    OrderRow {
        instance_id: r.get("instance_id"),
        item: r.get("item"),
        amount: r.get::<i64, _>("amount") as u32,
    }
}

pub async fn list() -> Result<Vec<OrderRow>, sqlx::Error> {
    let rows = sqlx::query("SELECT instance_id, item, amount FROM orders ORDER BY created_at DESC")
        .fetch_all(&pool())
        .await?;
    Ok(rows.iter().map(row_from).collect())
}

pub async fn get(instance_id: &str) -> Result<Option<OrderRow>, sqlx::Error> {
    let row = sqlx::query("SELECT instance_id, item, amount FROM orders WHERE instance_id = $1")
        .bind(instance_id)
        .fetch_optional(&pool())
        .await?;
    Ok(row.as_ref().map(row_from))
}
```

- [ ] **Step 7: Start Postgres and run the test to verify it passes**

Run: `docker compose up -d` (wait for healthy)
Run: `DATABASE_URL=postgres://duroxide:duroxide@localhost:5432/duroxide cargo test --features server --no-default-features orders -- --test-threads=1 --nocapture 2>&1 | tail -15`
Expected: PASS — `insert_get_list_roundtrip` green (migration auto-applied, roundtrip verified).

- [ ] **Step 8: Commit**

```bash
git add -A
git commit -m "feat: orders module with sqlx migration, reusing duroxide-pg pool"
```

---

## Task 2: Switch order storage to Postgres (workflow + server + UI)

**Files:**
- Modify: `src/workflow.rs`, `src/server.rs`, `src/app.rs`

**Interfaces:**
- Consumes: `orders::{init, insert, list, get, OrderRow}` from Task 1; existing `workflow::{client, stage_from_status, ORCHESTRATION_NAME, APPROVAL_EVENT}`.
- Produces:
  - `workflow::init` now also calls `orders::init(provider.pool().clone())`.
  - `server::OrderStatusDto { instance_id: String, item: String, amount: u32, stage: String, actionable: bool }`.

- [ ] **Step 1: Rewire `workflow::init` to reuse the pool; remove the in-memory registry**

In `src/workflow.rs`, replace the `ORDERS` static and the `record_order`/`all_orders` functions AND the `init` function body. Delete these items:
```rust
static ORDERS: Mutex<Vec<String>> = Mutex::new(Vec::new());

pub fn record_order(instance_id: &str) {
    ORDERS.lock().unwrap().push(instance_id.to_string());
}

pub fn all_orders() -> Vec<String> {
    ORDERS.lock().unwrap().clone()
}
```
Also remove the now-unused `Mutex` from the `use std::sync::{...}` import (leave `Arc`, `OnceLock`).

Replace `init` with (get the pool BEFORE moving the provider into the Arc):
```rust
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
```

- [ ] **Step 2: Update `src/server.rs` — DTO gains item/amount; use the orders store**

Replace the `OrderStatusDto` struct and the `start_order`, `get_order_status`, `list_orders` functions:
```rust
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct OrderStatusDto {
    pub instance_id: String,
    pub item: String,
    pub amount: u32,
    pub stage: String,
    pub actionable: bool,
}

#[server]
pub async fn start_order(order: OrderInput) -> ServerFnResult<String> {
    use crate::{orders, workflow};
    let instance_id = format!("order-{}", uuid::Uuid::new_v4());
    let input = serde_json::to_string(&order).map_err(ServerFnError::new)?;
    workflow::client()
        .start_orchestration(instance_id.clone(), workflow::ORCHESTRATION_NAME, input)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;
    orders::insert(&instance_id, &order.item, order.amount)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;
    Ok(instance_id)
}

#[server]
pub async fn get_order_status(instance_id: String) -> ServerFnResult<OrderStatusDto> {
    use crate::{orders, workflow};
    let row = orders::get(&instance_id)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?
        .ok_or_else(|| ServerFnError::new(format!("order {instance_id} not found")))?;
    let status = workflow::client()
        .get_orchestration_status(&instance_id)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;
    let (stage, actionable) = workflow::stage_from_status(&status);
    Ok(OrderStatusDto { instance_id: row.instance_id, item: row.item, amount: row.amount, stage, actionable })
}

#[server]
pub async fn list_orders() -> ServerFnResult<Vec<OrderStatusDto>> {
    use crate::{orders, workflow};
    let client = workflow::client();
    let rows = orders::list().await.map_err(|e| ServerFnError::new(e.to_string()))?;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let status = client
            .get_orchestration_status(&row.instance_id)
            .await
            .map_err(|e| ServerFnError::new(e.to_string()))?;
        let (stage, actionable) = workflow::stage_from_status(&status);
        out.push(OrderStatusDto { instance_id: row.instance_id, item: row.item, amount: row.amount, stage, actionable });
    }
    Ok(out)
}
```
(`submit_decision` is unchanged.)

- [ ] **Step 3: Update `src/app.rs` — Item/Amount columns**

In `App`, change the table header row to:
```rust
            tr { th { "Item" } th { "Amount" } th { "Instance" } th { "Stage" } th { "Action" } }
```
In `OrderRow`, add the two cells before the instance cell:
```rust
        tr {
            td { "{order.item}" }
            td { "{order.amount}" }
            td { class: "mono", "{order.instance_id}" }
            td {
                span { class: stage_class(&order.stage), "{order.stage}" }
            }
```
(Leave the Action `td` and the rest of `OrderRow` as-is.)

- [ ] **Step 4: Replace the workflow tests with Postgres-backed approve/reject**

In `src/workflow.rs`, replace the ENTIRE `#[cfg(test)] mod tests { ... }` block (removing the `SqliteProvider` import and all SQLite/PG-gated tests) with:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use duroxide_pg::PostgresProvider;

    // Full approve/reject against real Postgres (requires DATABASE_URL + docker).
    // Isolated by unique instance ids; run with --test-threads=1.
    async fn run(item: &str, amount: u32, decision: &str) -> (OrchestrationStatus, crate::orders::OrderRow) {
        let url = std::env::var("DATABASE_URL").expect("DATABASE_URL (docker postgres) required");
        let provider = PostgresProvider::new(&url).await.unwrap();
        let pool = provider.pool().clone();
        crate::orders::init(pool).await.unwrap();

        let store: Arc<dyn Provider> = Arc::new(provider);
        let (activities, orchestrations) = registries();
        let rt = Runtime::start_with_store(store.clone(), activities, orchestrations).await;
        std::mem::forget(rt);
        let client = Arc::new(Client::new(store));

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
    async fn approve_persists_order_and_fulfills() {
        let (status, row) = run("Widget", 10, "approve").await;
        assert_eq!(row.item, "Widget");
        assert_eq!(row.amount, 10);
        assert!(
            matches!(&status, OrchestrationStatus::Completed { output, .. } if output.contains("FULFILLED")),
            "got {status:?}"
        );
    }

    #[tokio::test]
    async fn reject_persists_order_and_refunds() {
        let (status, row) = run("Gadget", 42, "reject").await;
        assert_eq!(row.item, "Gadget");
        assert_eq!(row.amount, 42);
        assert!(
            matches!(&status, OrchestrationStatus::Completed { output, .. } if output.contains("REFUNDED")),
            "got {status:?}"
        );
    }
}
```

- [ ] **Step 5: Verify both targets compile**

Run: `cargo check --features server --no-default-features 2>&1 | tail -8`
Expected: PASS.
Run: `cargo check --features web --no-default-features 2>&1 | tail -8`
Expected: PASS (server bodies compile out; `OrderStatusDto` with new fields still used by the UI).

- [ ] **Step 6: Run the Postgres-backed tests**

Run: `docker compose up -d` (if not already running)
Run: `DATABASE_URL=postgres://duroxide:duroxide@localhost:5432/duroxide cargo test --features server --no-default-features -- --test-threads=1 2>&1 | tail -20`
Expected: PASS — `orders::tests::insert_get_list_roundtrip`, `workflow::tests::approve_persists_order_and_fulfills`, `workflow::tests::reject_persists_order_and_refunds` all green.

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m "feat: store orders in postgres via sqlx; join with live duroxide status"
```

---

## Task 3: Docs + end-to-end verification

**Files:**
- Modify: `README.md`, `docs/API-NOTES.md`

**Interfaces:**
- Consumes: everything from Tasks 1–2.

- [ ] **Step 1: Update `docs/API-NOTES.md`**

Append a section:
```markdown
## duroxide-pg pool reuse & app storage (sqlx)
- `PostgresProvider::pool(&self) -> &PgPool` (provider.rs:511) — Arc-backed; `provider.pool().clone()`
  shares the same pool. Used by `orders::init`.
- duroxide-pg does NOT set a persistent `search_path`; it fully-qualifies its own queries and uses
  `SET LOCAL` only inside its migration txn. So the app's `orders` table (unqualified) lives in the
  pool's default schema (`public` for `PostgresProvider::new`). No conflict with duroxide tables.
- App schema is a sqlx migration: `migrations/0001_create_orders.sql`, run via
  `sqlx::migrate!("./migrations").run(&pool)`. sqlx tracks state in `_sqlx_migrations`
  (distinct from duroxide's `_duroxide_migrations`).
- sqlx 0.8.6, runtime query API (no `query!` macro → no build-time DB needed).
```

- [ ] **Step 2: Update `README.md` — replace the Tests section and the in-memory note**

Replace the `## Tests` section body with:
```markdown
Tests run against real Postgres (start it first with `docker compose up -d`):

​```bash
DATABASE_URL=postgres://duroxide:duroxide@localhost:5432/duroxide \
  cargo test --features server --no-default-features -- --test-threads=1
​```

- `orders::insert_get_list_roundtrip` — the sqlx orders store.
- `workflow::approve_persists_order_and_fulfills` / `reject_persists_order_and_refunds` —
  the full orchestration against Postgres, asserting the persisted order row and the approve →
  **FULFILLED** / reject → **REFUNDED** outcomes.

Tests isolate via unique instance ids and run serially.
```
Then, in the "Durability check" section, replace the parenthetical about the in-memory id list with:
```markdown
(Orders are now stored in the `orders` table in Postgres, so the dashboard list also survives
restart — not just the workflow execution state.)
```

- [ ] **Step 3: End-to-end verification against the running server**

Run: `docker compose up -d`
Run (background): `DATABASE_URL=postgres://duroxide:duroxide@localhost:5432/duroxide IP=127.0.0.1 PORT=8099 cargo run --features server --no-default-features &`
Wait for `listening on http://127.0.0.1:8099`.
Discover endpoint hash: `grep -aoE "/api/start_order[0-9]+" target/debug/deps/*.wasm 2>/dev/null | head -1` — OR reuse the hash printed by a prior `dx build` (the server registers the same `/api/<name><hash>` paths). If unsure, run `dx build --platform web` and `grep -aoE "/api/[a-z_]+[0-9]+" target/dx/duroxus/debug/web/public/wasm/*.wasm | sort -u`.

With `H` = the hash and `B=http://127.0.0.1:8099/api`:
```bash
ID=$(curl -s -X POST "$B/start_order$H" -H 'Content-Type: application/json' -d '{"order":{"item":"Widget","amount":10}}' | tr -d '"')
until curl -s -X POST "$B/list_orders$H" -H 'Content-Type: application/json' -d '{}' | grep -q "$ID.*Awaiting"; do sleep 1; done
curl -s -X POST "$B/submit_decision$H" -H 'Content-Type: application/json' -d "{\"instance_id\":\"$ID\",\"approve\":true}"
until curl -s -X POST "$B/list_orders$H" -H 'Content-Type: application/json' -d '{}' | grep -q "$ID.*Fulfilled"; do sleep 1; done
curl -s -X POST "$B/list_orders$H" -H 'Content-Type: application/json' -d '{}'
```
Expected: the final JSON row includes `"item":"Widget"`, `"amount":10`, `"stage":"Fulfilled"`.

Durability: restart the app process (Ctrl-C, re-run) and `curl .../list_orders$H` again — the Widget order is still listed (now loaded from Postgres, not memory).

Then stop the server process.

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "docs: postgres order-store notes + updated test/e2e instructions"
```

---

## Self-Review

**Spec coverage:**
- Reuse duroxide-pg pool (`provider.pool().clone()`) → Task 2 Step 1. ✓
- sqlx dep 0.8 + migrate feature; remove sqlite/test-support → Task 1 Step 1. ✓
- Migration file + `sqlx::migrate!` run at startup → Task 1 Steps 2, 6; wired in Task 2 Step 1. ✓
- `orders` table schema/columns → Task 1 Step 2. ✓
- orders API (init/pool/insert/list/get, OrderRow) → Task 1 Step 6. ✓
- Remove in-memory Vec + record_order/all_orders → Task 2 Step 1. ✓
- start_order inserts; list/get join with live status; DTO gains item/amount → Task 2 Steps 1–2. ✓
- UI Item/Amount columns → Task 2 Step 3. ✓
- PG-backed tests (unique ids, serial, docker) replacing SQLite → Task 1 Step 4/7, Task 2 Step 4/6. ✓
- Docs (README, API-NOTES) → Task 3. ✓

**Deviation from spec (intentional, justified):** The spec proposed a dedicated `duroxus_test` schema
for test isolation. Investigation showed duroxide-pg does not set a persistent `search_path`, so the
unqualified `orders`/`_sqlx_migrations` would land in `public` regardless — the schema would not
isolate them. Tests instead isolate via **unique instance ids** in `public` (asserting only on their
own ids), which is simpler and exercises the app's real configuration. Behavior verified is identical.

**Placeholder scan:** No TBD/TODO; every code step is complete. The e2e hash-discovery step gives
concrete commands (the `/api/<name><hash>` scheme is confirmed from the prior build).

**Type consistency:** `OrderRow { instance_id: String, item: String, amount: u32 }` defined in Task 1
and consumed unchanged in Task 2 (server.rs, workflow tests). `OrderStatusDto` fields
(`instance_id, item, amount, stage, actionable`) defined in Task 2 Step 2 and consumed in Task 2
Step 3 (app.rs). `orders::{init, pool, insert, list, get}` signatures match all call sites.
`amount` is `u32` end-to-end, bound/read as `i64` at the sqlx boundary only. ✓
