# Confirmed duroxide API (installed versions)

**duroxide:** `0.1.29`   **duroxide-pg:** `0.1.34`

Source of truth for the implementation. Grepped from
`~/.cargo/registry/src/index.crates.io-*/duroxide-0.1.29` and `duroxide-pg-0.1.34`.

## Client  (`duroxide::Client`, re-exported at crate root)
- `Client::new(store: Arc<dyn duroxide::providers::Provider>) -> Client`
- `async start_orchestration(instance: impl Into<String>, orchestration: impl Into<String>, input: impl Into<String>) -> Result<(), ClientError>`
- `async raise_event(instance: impl Into<String>, event_name: impl Into<String>, data: impl Into<String>) -> Result<(), ClientError>`
- `async get_orchestration_status(instance: &str) -> Result<OrchestrationStatus, ClientError>`  ← **note the name** (not `get_status`)
- `async wait_for_orchestration(instance: &str, timeout: Duration) -> Result<OrchestrationStatus, ClientError>`
- `ClientError` implements `Display`; variants include `Provider(..)`, `Timeout`.

## OrchestrationContext
- `schedule_activity(name: impl Into<String>, input: impl Into<String>) -> DurableFuture<Result<String, String>>`
- `schedule_timer(delay: Duration) -> DurableFuture<()>`
- `schedule_wait(name: impl Into<String>) -> DurableFuture<String>`
- `async select2<T1,T2,F1,F2>(&self, f1: F1, f2: F2) -> Either2<T1, T2>` — pass the two `DurableFuture`s directly.
- `Either2::{First(A), Second(B)}` (crate root: `duroxide::Either2`).

## Runtime  (`duroxide::runtime::Runtime`)
- `async Runtime::start_with_store(history_store: Arc<dyn Provider>, activity_registry: ActivityRegistry, orchestration_registry: OrchestrationRegistry) -> Arc<Runtime>`
- Keep the returned `Arc<Runtime>` alive for the process.

## Registries  (re-exported: `duroxide::{OrchestrationRegistry}`; `ActivityRegistry` via `duroxide::runtime::registry::ActivityRegistry` — also re-exported at `duroxide::ActivityRegistry`? use `duroxide::runtime::...` if root fails)
- `type OrchestrationRegistry = Registry<dyn OrchestrationHandler>`
- `type ActivityRegistry = Registry<dyn ActivityHandler>`
- Builder: `ActivityRegistry::builder().register(name, f).build()` / `OrchestrationRegistry::builder().register(name, f).build()`
- Handler closures: `|ctx: ActivityContext, input: String| async move { Ok::<String,String>(..) }`
  and `|ctx: OrchestrationContext, input: String| async move { Ok::<String,String>(..) }`.

## OrchestrationStatus  (`duroxide::OrchestrationStatus`) — **variants differ from the plan draft**
```rust
enum OrchestrationStatus {
    NotFound,
    Running   { custom_status: Option<String>, custom_status_version: u64 },
    Completed { output: String, custom_status: Option<String>, custom_status_version: u64 },
    Failed    { details: ErrorDetails, custom_status: Option<String>, custom_status_version: u64 },
}
```
- No `Pending` variant. `Failed` carries `details: ErrorDetails` (not `error: String`); use `details.display_message()`.

## PostgresProvider  (`duroxide_pg::PostgresProvider`)
- `async PostgresProvider::new(database_url: &str) -> anyhow::Result<Self>`
- `async PostgresProvider::new_with_schema(database_url: &str, schema_name: Option<&str>) -> anyhow::Result<Self>`
- `impl Provider for PostgresProvider` — wrap as `Arc::new(provider) as Arc<dyn Provider>`.

## SqliteProvider (tests only)  (`duroxide::providers::sqlite::SqliteProvider`)
- **Feature-gated:** requires the duroxide `sqlite` feature (`sqlite = ["sqlx","libsqlite3-sys"]`; duroxide default features are empty).
- `async SqliteProvider::new_in_memory() -> Result<Self, sqlx::Error>`
- `async SqliteProvider::new(database_url: &str, options: Option<SqliteOptions>) -> Result<Self, sqlx::Error>`

### Consequence for Cargo.toml (historical)
The SQLite provider was originally used for tests via a `test-support = ["server", "duroxide/sqlite"]`
feature. That was **removed** once orders moved to Postgres — tests now require Docker Postgres (below).

## duroxide-pg pool reuse & app storage (sqlx)
- `PostgresProvider::pool(&self) -> &PgPool` (provider.rs:511) — Arc-backed; `provider.pool().clone()`
  shares the SAME pool. Used by `orders::init` so the app has one pool for both duroxide and orders.
- Pool config: `max_connections` defaults to 10 (override via `DUROXIDE_PG_POOL_MAX`), `acquire_timeout`
  30s. Raw connection-count is rarely the bottleneck; runtime contention is (see below).
- duroxide-pg does NOT set a persistent `search_path`; it fully-qualifies its own queries and uses
  `SET LOCAL` only inside its migration txn. So the app's `orders` table (unqualified) lives in the
  pool's default schema (`public` for `PostgresProvider::new`). No conflict with duroxide tables.
- App schema is a sqlx migration: `migrations/0001_create_orders.sql`, run via
  `sqlx::migrate!("./migrations").run(&pool)`. sqlx tracks state in `_sqlx_migrations`
  (distinct from duroxide's `_duroxide_migrations`).
- sqlx 0.8.6, runtime query API (no `query!` macro → no build-time DB needed).

## Runtime lifecycle (important for tests)
- `Runtime::start_with_store(...) -> Arc<Runtime>` spawns background dispatcher tasks. **Dropping the
  `Arc` does NOT stop them** — call `rt.shutdown(Some(timeout_ms)).await` (runtime/mod.rs:1010).
- duroxide expects ONE runtime per store/schema. Multiple runtimes over the same schema/queues
  contend and deadlock (observed as `database is deadlocked` retries → `PoolTimedOut`).
- Consequence for tests: each `#[tokio::test]` gets its own tokio runtime, and a sqlx pool / duroxide
  dispatchers are bound to their creating runtime — they can't be shared across test fns. So the PG
  integration lives in ONE test (`workflow::tests::postgres_order_lifecycle`) with a single runtime.

### Test command
`DATABASE_URL=postgres://duroxide:duroxide@localhost:5432/duroxide cargo test --features server --no-default-features -- --test-threads=1`
