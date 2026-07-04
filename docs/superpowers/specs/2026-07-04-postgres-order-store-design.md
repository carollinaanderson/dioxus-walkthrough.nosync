# Postgres-backed order store (reusing duroxide-pg's pool)

**Date:** 2026-07-04
**Status:** Approved (design)
**Builds on:** [2026-07-04-dioxus-duroxide-poc-design.md](./2026-07-04-dioxus-duroxide-poc-design.md)

## Goal

Persist order business-data (item, amount, created_at) in PostgreSQL via `sqlx`, replacing the
in-memory `Mutex<Vec<String>>` dashboard registry. Reuse duroxide-pg's connection pool. The schema
is applied through sqlx's migration system. Tests move from the in-memory SQLite provider to real
Postgres (Docker required).

## Key facts (pinned)

- `duroxide_pg::PostgresProvider` exposes `pub fn pool(&self) -> &PgPool` (provider.rs:511), backed
  by an internal `Arc<PgPool>`. `PgPool` is cheaply cloneable (Arc inside), so `provider.pool().clone()`
  yields a handle to the **same** pool — one connection budget, no second pool.
- duroxide-pg uses **sqlx 0.8.6**. We depend on `sqlx = "0.8"` so the `PgPool` type is shared.
- duroxide-pg tracks its own migrations in `_duroxide_migrations`; sqlx uses `_sqlx_migrations`.
  No collision.

## Architecture

New server-only module `src/orders.rs` owns the `orders` table and all its queries.

- `workflow::init` builds the `PostgresProvider`, then calls `orders::init(provider.pool().clone())`
  **before** setting the duroxide `Client`. `orders::init` stashes the pool in a
  `static POOL: OnceLock<PgPool>` (mirroring the existing `CLIENT` global) and runs the sqlx
  migrations against it.
- The in-memory `ORDERS: Mutex<Vec<String>>` and the `record_order` / `all_orders` functions are
  removed from `workflow.rs`.

## Migration (idiomatic sqlx)

- Directory `migrations/` at the crate root with:
  - `migrations/0001_create_orders.sql`:
    ```sql
    CREATE TABLE IF NOT EXISTS orders (
      instance_id TEXT PRIMARY KEY,
      item        TEXT        NOT NULL,
      amount      BIGINT      NOT NULL,
      created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
    );
    ```
- `orders.rs` embeds them at compile time: `static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");`
  and `orders::init` runs `MIGRATOR.run(&pool).await`.
- Requires the sqlx `migrate` feature. Migrations are applied idempotently (sqlx skips already-applied
  versions via `_sqlx_migrations`). The table lands in the pool's active schema/search-path (default
  `public` for `PostgresProvider::new`; the dedicated test schema for `new_with_schema`).

## Data flow (business data in table, execution state in duroxide — joined at read)

- **start_order(order)**: `client.start_orchestration(id, …)` → `orders::insert(id, item, amount)`
  (`INSERT INTO orders …`). Returns the instance id.
- **list_orders()**: `orders::list()` → `SELECT instance_id, item, amount FROM orders ORDER BY created_at DESC`;
  for each row, query live `client.get_orchestration_status` → `stage_from_status` → build DTO.
  Durable across restart.
- **get_order_status(id)**: `orders::get(id)` + live status → DTO. Errors if the row is absent.
- **submit_decision(id, approve)**: unchanged (raises the `approval` external event).

## Interfaces

- `OrderStatusDto` gains `item: String` and `amount: u32`.
- `orders.rs` public API:
  - `async fn init(pool: PgPool) -> Result<(), String>` — stash pool + run migrations.
  - `fn pool() -> PgPool` — clone of the stored pool; panics if `init` not called.
  - `async fn insert(instance_id: &str, item: &str, amount: u32) -> Result<(), sqlx::Error>`
  - `async fn list() -> Result<Vec<OrderRow>, sqlx::Error>`
  - `async fn get(instance_id: &str) -> Result<Option<OrderRow>, sqlx::Error>`
  - `struct OrderRow { instance_id: String, item: String, amount: u32 }`
- UI table columns: **Item · Amount · Stage · Action**.

## Error handling

- `orders::init` maps sqlx/migration errors to `String` (consumed by `workflow::init`).
- Server functions map `sqlx::Error` and `ClientError` to `ServerFnError` via `.to_string()`.
- `get_order_status` returns a `ServerFnError` when the order row is missing.

## Testing (Docker Postgres required)

- Remove the duroxide `sqlite` feature, the `test-support` feature, and the two in-memory tests.
- New Postgres-backed tests in `orders.rs` (or `workflow.rs`):
  - Use a dedicated schema via `PostgresProvider::new_with_schema(url, Some("duroxus_test"))` so
    duroxide tables, the sqlx `orders` table, and `_sqlx_migrations` are isolated from `public`.
  - Build the pool from that provider, run `orders::init`, register duroxide, start orchestrations
    with **unique instance ids per test**, and assert on those specific ids (so a shared table needs
    no truncation).
  - Cases: start+approve → `orders::list()` contains the row with correct `item`/`amount` and the
    live stage resolves to `Fulfilled`; start+reject → stage `Refunded`.
  - Run serially (`--test-threads=1`) to avoid duroxide's shared-store contention warnings.
- Command: `DATABASE_URL=postgres://duroxide:duroxide@localhost:5432/duroxide cargo test --features server --no-default-features -- --test-threads=1`

## Cargo.toml

- Add: `sqlx = { version = "0.8", features = ["runtime-tokio", "postgres", "migrate"], optional = true }`
  to the `server` feature.
- Remove: the `test-support` feature and `duroxide/sqlite`.

## Files

- **New:** `src/orders.rs`, `migrations/0001_create_orders.sql`.
- **Modify:** `src/workflow.rs` (pool wiring in `init`, drop `ORDERS`/`record_order`/`all_orders`,
  replace tests), `src/server.rs` (DTO + queries), `src/app.rs` (table columns), `src/main.rs`
  (`mod orders`), `Cargo.toml`, `README.md`, `docs/API-NOTES.md` (add `pool()` + sqlx-migrate notes).

## Out of scope (YAGNI)

- No status/stage column mirrored into `orders` (duroxide remains the single source of execution
  state). No update/delete of orders, no pagination, no per-user scoping.
