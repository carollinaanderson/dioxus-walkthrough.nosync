# graphile_worker + Auth Refactor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace duroxide durable orchestration with graphile_worker_rs background jobs, add session-based auth with login/register/orders pages, give server functions named HTTP endpoints, and rewrite the README with a researched deployment section.

**Architecture:** Single binary: Dioxus fullstack server + an embedded graphile_worker worker spawned as a background tokio task, all against one Postgres (app tables + graphile_worker schema + sessions table). Orders run a chained job pipeline `ValidateOrder → ChargePayment → FulfillOrder` that updates `orders.status`; the UI polls. Auth is tower-sessions (Postgres store) + argon2-hashed users; every order server fn requires a session.

**Tech Stack:** Rust, Dioxus 0.7 (fullstack + router), axum 0.8, sqlx 0.8 (Postgres), graphile_worker 0.13, tower-sessions 0.15 + tower-sessions-sqlx-store 0.15, argon2 0.5.

**Spec:** `docs/superpowers/specs/2026-07-07-graphile-worker-auth-refactor-design.md`

## Global Constraints

- Crate versions: `graphile_worker = "0.13"`, `tower-sessions = "0.15"`, `tower-sessions-sqlx-store = { version = "0.15", features = ["postgres"] }`, `argon2 = "0.5"` (NOT 0.6 — that's an RC), keep `dioxus = "0.7"`, `axum = "0.8"`, `sqlx = "0.8"`.
- All server-only deps stay `optional = true`, activated by the `server` feature; client (wasm) build must keep compiling with default features.
- duroxide and duroxide-pg are removed entirely — no references may remain.
- Order statuses are exactly: `queued | validating | charging | fulfilling | fulfilled | failed` (lowercase strings).
- Named endpoints: `auth/register`, `auth/login`, `auth/logout`, `auth/me`, `orders/start`, `orders/list`, `orders/{id}` — all under `/api/`.
- The unauthenticated error message is exactly the string `unauthenticated` (the UI matches on it).
- Postgres for dev/test comes from the existing `docker-compose.yml` (`postgres://duroxide:duroxide@localhost:5432/duroxide`). Schema changed → tell the user to `docker compose down -v && docker compose up -d` before first run.
- Verification commands: `cargo check` (client), `cargo check --features server` (server), `cargo test --features server` (needs Postgres up).
- API-shape fallbacks (only if compile fails): `WorkerUtils` may live at `graphile_worker::worker_utils::WorkerUtils`; `JobSpec` at `graphile_worker::JobSpec`. `ctx.pg_pool()` requires graphile_worker's sqlx integration (it is the default). Check `cargo doc -p graphile_worker --no-deps --open` before improvising.

---

### Task 1: Replace duroxide with graphile_worker + auth backend

**Files:**
- Modify: `Cargo.toml`
- Delete: `src/workflow.rs`, `docs/API-NOTES.md`, `migrations/0001_create_orders.sql`
- Create: `migrations/0001_create_users.sql`, `migrations/0002_create_orders.sql`
- Create: `src/users.rs`, `src/jobs.rs`, `src/auth.rs`
- Rewrite: `src/orders.rs`, `src/state.rs`, `src/main.rs`, `src/server.rs`
- Modify: `src/app.rs` (interim adaptation so the client target compiles; final UI is Task 2)
- Test: argon2 unit test inside `src/auth.rs` (`#[cfg(all(test, feature = "server"))]`)

**Interfaces:**
- Produces (used by later tasks):
  - `state::AppState { pool: PgPool, utils: Arc<WorkerUtils> }`, `AppState::new() -> (AppState, Worker)`
  - `users::{UserRow {id: Uuid, username: String, password_hash: String}, insert(pool, username, password_hash) -> Result<UserRow>, find_by_username(pool, username) -> Result<Option<UserRow>>, find_by_id(pool, id: Uuid) -> Result<Option<UserRow>>}`
  - `orders::{OrderRow {id: Uuid, user_id: Uuid, item: String, amount: i64, status: String}, init(pool), insert(pool, user_id, item, amount: u32) -> Result<OrderRow>, list_for_user(pool, user_id) -> Result<Vec<OrderRow>>, get_for_user(pool, user_id, id) -> Result<Option<OrderRow>>, set_status(pool, id, status)}`
  - `jobs::{ValidateOrder{order_id: Uuid}, ChargePayment{order_id: Uuid}, FulfillOrder{order_id: Uuid}}`
  - `server::{OrderInput{item: String, amount: u32}, OrderDto{id: String, item: String, amount: i64, status: String}, start_order, list_orders, get_order}`
  - `auth::CurrentUser { id: String, username: String }` (serde, Clone, PartialEq)
  - Auth server fns: `register(username: String, password: String) -> ServerFnResult<CurrentUser>`, `login(username: String, password: String) -> ServerFnResult<CurrentUser>`, `logout() -> ServerFnResult<()>`, `current_user() -> ServerFnResult<Option<CurrentUser>>`
  - `auth::require_user_id(&tower_sessions::Session) -> Result<uuid::Uuid, ServerFnError>` (server-only) — errors with message exactly `unauthenticated`; `auth::UNAUTHENTICATED: &str = "unauthenticated"`; `auth::hash_password(&str) -> Result<String, String>` (`pub(crate)`, server-only)

- [ ] **Step 1: Reset dev database (schema is being replaced)**

```bash
docker compose down -v && docker compose up -d && sleep 5
```

- [ ] **Step 2: Rewrite Cargo.toml dependencies**

Replace the `[dependencies]` and `[features]` sections of `Cargo.toml` with:

```toml
[dependencies]
dioxus = { version = "0.7", features = ["fullstack", "router"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"

# Server-only (not WASM-compatible)
axum = { version = "0.8", optional = true }
tokio = { version = "1", features = ["full"], optional = true }
graphile_worker = { version = "0.13", optional = true }
uuid = { version = "1", features = ["v4", "serde"], optional = true }
dotenvy = { version = "0.15", optional = true }
sqlx = { version = "0.8", features = ["runtime-tokio", "postgres", "migrate", "uuid"], optional = true }
tower-sessions = { version = "0.15", optional = true }
tower-sessions-sqlx-store = { version = "0.15", features = ["postgres"], optional = true }
argon2 = { version = "0.5", optional = true }

# Client-only: interval sleep for the status-polling loop.
[target.'cfg(target_arch = "wasm32")'.dependencies]
gloo-timers = { version = "0.3", features = ["futures"] }

[features]
default = ["web"]
web = ["dioxus/web"]
server = [
    "dioxus/server",
    "dep:axum",
    "dep:tokio",
    "dep:graphile_worker",
    "dep:uuid",
    "dep:dotenvy",
    "dep:sqlx",
    "dep:tower-sessions",
    "dep:tower-sessions-sqlx-store",
    "dep:argon2",
]
```

Then run `cargo update -p graphile_worker 2>/dev/null; cargo tree -e features -i sqlx --features server | head -20` after Step 3 to confirm graphile_worker and our sqlx resolve to a single sqlx 0.8.x. If they diverge, pin `sqlx` to the exact minor graphile_worker 0.13 uses.

- [ ] **Step 3: Delete duroxide files, write migrations**

```bash
rm src/workflow.rs docs/API-NOTES.md migrations/0001_create_orders.sql
```

Create `migrations/0001_create_users.sql`:

```sql
CREATE TABLE IF NOT EXISTS users (
    id            UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    username      TEXT        NOT NULL UNIQUE,
    password_hash TEXT        NOT NULL,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);
```

Create `migrations/0002_create_orders.sql`:

```sql
CREATE TABLE IF NOT EXISTS orders (
    id         UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id    UUID        NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    item       TEXT        NOT NULL,
    amount     BIGINT      NOT NULL,
    status     TEXT        NOT NULL DEFAULT 'queued',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS orders_user_created_idx ON orders (user_id, created_at DESC);
```

- [ ] **Step 4: Create `src/users.rs`**

```rust
//! Server-only Postgres store for user accounts.

use sqlx::{prelude::FromRow, PgPool};
use uuid::Uuid;

#[derive(Debug, Clone, FromRow)]
pub struct UserRow {
    pub id: Uuid,
    pub username: String,
    pub password_hash: String,
}

pub async fn insert(
    pool: &PgPool,
    username: &str,
    password_hash: &str,
) -> Result<UserRow, sqlx::Error> {
    sqlx::query_as::<_, UserRow>(
        "INSERT INTO users (username, password_hash) VALUES ($1, $2)
         RETURNING id, username, password_hash",
    )
    .bind(username)
    .bind(password_hash)
    .fetch_one(pool)
    .await
}

pub async fn find_by_username(
    pool: &PgPool,
    username: &str,
) -> Result<Option<UserRow>, sqlx::Error> {
    sqlx::query_as::<_, UserRow>(
        "SELECT id, username, password_hash FROM users WHERE username = $1",
    )
    .bind(username)
    .fetch_optional(pool)
    .await
}

pub async fn find_by_id(pool: &PgPool, id: Uuid) -> Result<Option<UserRow>, sqlx::Error> {
    sqlx::query_as::<_, UserRow>("SELECT id, username, password_hash FROM users WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await
}
```

- [ ] **Step 5: Rewrite `src/orders.rs`**

```rust
//! Server-only Postgres store for order business-data. Schema is applied via
//! the sqlx migrations in `migrations/`. Orders belong to a user and carry a
//! `status` driven by the graphile_worker job pipeline in `jobs.rs`.

use sqlx::{migrate::MigrateError, prelude::FromRow, PgPool};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, FromRow)]
pub struct OrderRow {
    pub id: Uuid,
    pub user_id: Uuid,
    pub item: String,
    pub amount: i64,
    pub status: String,
}

/// Apply pending sqlx migrations against the (shared) pool.
pub async fn init(pool: &PgPool) -> Result<(), MigrateError> {
    sqlx::migrate!("./migrations").run(pool).await
}

pub async fn insert(
    pool: &PgPool,
    user_id: Uuid,
    item: &str,
    amount: u32,
) -> Result<OrderRow, sqlx::Error> {
    sqlx::query_as::<_, OrderRow>(
        "INSERT INTO orders (user_id, item, amount) VALUES ($1, $2, $3)
         RETURNING id, user_id, item, amount, status",
    )
    .bind(user_id)
    .bind(item)
    .bind(amount as i64)
    .fetch_one(pool)
    .await
}

pub async fn list_for_user(pool: &PgPool, user_id: Uuid) -> Result<Vec<OrderRow>, sqlx::Error> {
    sqlx::query_as::<_, OrderRow>(
        "SELECT id, user_id, item, amount, status FROM orders
         WHERE user_id = $1 ORDER BY created_at DESC",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
}

pub async fn get_for_user(
    pool: &PgPool,
    user_id: Uuid,
    id: Uuid,
) -> Result<Option<OrderRow>, sqlx::Error> {
    sqlx::query_as::<_, OrderRow>(
        "SELECT id, user_id, item, amount, status FROM orders WHERE id = $1 AND user_id = $2",
    )
    .bind(id)
    .bind(user_id)
    .fetch_optional(pool)
    .await
}

pub async fn set_status(pool: &PgPool, id: Uuid, status: &str) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE orders SET status = $2 WHERE id = $1")
        .bind(id)
        .bind(status)
        .execute(pool)
        .await?;
    Ok(())
}
```

- [ ] **Step 6: Create `src/jobs.rs`**

```rust
//! graphile_worker task handlers for the order pipeline:
//! ValidateOrder -> ChargePayment -> FulfillOrder.
//!
//! Each handler stamps its stage onto `orders.status`, simulates work with a
//! short sleep (so the polling UI visibly walks the stages), then enqueues the
//! next job. Any error marks the order `failed` before surfacing the error to
//! graphile_worker.

use std::time::Duration;

use graphile_worker::{IntoTaskHandlerResult, JobSpec, TaskHandler, WorkerContext, WorkerUtils};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

const STEP_DELAY: Duration = Duration::from_millis(1200);

async fn set_stage(ctx: &WorkerContext, order_id: Uuid, stage: &str) -> Result<(), String> {
    crate::orders::set_status(ctx.pg_pool(), order_id, stage)
        .await
        .map_err(|e| e.to_string())
}

async fn enqueue<T: TaskHandler>(ctx: &WorkerContext, job: T) -> Result<(), String> {
    let utils = WorkerUtils::new(ctx.pg_pool().clone(), ctx.escaped_schema().to_string());
    utils
        .add_job(job, JobSpec::default())
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Mark the order failed if a step errored, passing the error through.
async fn or_fail(ctx: &WorkerContext, order_id: Uuid, res: Result<(), String>) -> Result<(), String> {
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
```

Note: `ctx.pg_pool()` returns the worker's sqlx `PgPool` (graphile_worker ships sqlx integration by default). If `WorkerUtils` is not at the crate root, import `graphile_worker::worker_utils::WorkerUtils`.

- [ ] **Step 7: Rewrite `src/state.rs`**

```rust
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
```

If `WorkerOptions` has no `pg_pool` method under that name, check the builder docs — the option is documented as `database` / `pg_pool`; use whichever method exists rather than falling back to `database_url` (we want one shared pool).

- [ ] **Step 8: Rewrite `src/main.rs`**

```rust
mod app;
mod auth;
mod server;
use app::App;

#[cfg(feature = "server")]
mod jobs;

#[cfg(feature = "server")]
mod orders;

#[cfg(feature = "server")]
mod state;

#[cfg(feature = "server")]
mod users;

fn main() {
    // Client entrypoint.
    #[cfg(not(feature = "server"))]
    dioxus::launch(App);

    // Server entrypoint: connect Postgres, run migrations, spawn the embedded
    // graphile_worker worker, and serve the app with session + state layers.
    #[cfg(feature = "server")]
    dioxus::serve(|| async {
        let (state, worker) = state::AppState::new().await;
        tokio::spawn(async move {
            if let Err(e) = worker.run().await {
                eprintln!("graphile_worker exited: {e}");
            }
        });

        let session_store =
            tower_sessions_sqlx_store::PostgresStore::new(state.pool.clone());
        session_store
            .migrate()
            .await
            .expect("failed to migrate session store");
        // `with_secure(false)` so the cookie works over plain http in dev;
        // set it to true behind TLS in production.
        let session_layer = tower_sessions::SessionManagerLayer::new(session_store)
            .with_secure(false);

        Ok(dioxus::server::router(App)
            .layer(session_layer)
            .layer(axum::Extension(state)))
    });
}
```

`mod auth;` is unconditional — like `mod server;`, the `#[server]` macros in it must compile on both targets (Step 9 creates it).

- [ ] **Step 9: Create `src/auth.rs` (auth server fns + argon2 helpers + unit test)**

```rust
//! Auth `#[server]` functions and session helpers: register, login, logout,
//! current_user. Sessions are tower-sessions backed by Postgres (layered in
//! `main.rs`); passwords are argon2-hashed. Order server fns call
//! [`require_user_id`] to enforce authentication server-side.

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct CurrentUser {
    pub id: String,
    pub username: String,
}

/// Session key holding the logged-in user's uuid.
pub const SESSION_USER_KEY: &str = "user_id";

/// Error message for unauthenticated requests. The UI matches on this string
/// to redirect to the login page — keep it in sync with `pages/orders.rs`.
pub const UNAUTHENTICATED: &str = "unauthenticated";

/// Extract the logged-in user's id from the session, or fail with
/// [`UNAUTHENTICATED`]. This is the server-side auth boundary for all
/// protected server fns.
#[cfg(feature = "server")]
pub async fn require_user_id(
    session: &tower_sessions::Session,
) -> Result<uuid::Uuid, ServerFnError> {
    session
        .get::<uuid::Uuid>(SESSION_USER_KEY)
        .await
        .ok()
        .flatten()
        .ok_or_else(|| ServerFnError::new(UNAUTHENTICATED))
}

#[cfg(feature = "server")]
pub(crate) fn hash_password(password: &str) -> Result<String, String> {
    use argon2::password_hash::{rand_core::OsRng, SaltString};
    use argon2::{Argon2, PasswordHasher};
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| e.to_string())
}

#[cfg(feature = "server")]
fn verify_password(password: &str, hash: &str) -> bool {
    use argon2::{Argon2, PasswordHash, PasswordVerifier};
    PasswordHash::new(hash)
        .map(|parsed| {
            Argon2::default()
                .verify_password(password.as_bytes(), &parsed)
                .is_ok()
        })
        .unwrap_or(false)
}

#[post("/api/auth/register", state: axum::Extension<crate::state::AppState>, session: tower_sessions::Session)]
pub async fn register(username: String, password: String) -> ServerFnResult<CurrentUser> {
    let username = username.trim().to_string();
    if username.is_empty() {
        return Err(ServerFnError::new("username is required"));
    }
    if password.len() < 8 {
        return Err(ServerFnError::new("password must be at least 8 characters"));
    }
    if crate::users::find_by_username(&state.pool, &username)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?
        .is_some()
    {
        return Err(ServerFnError::new("username already taken"));
    }
    let hash = hash_password(&password).map_err(ServerFnError::new)?;
    let user = crate::users::insert(&state.pool, &username, &hash)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;
    session
        .insert(SESSION_USER_KEY, user.id)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;
    Ok(CurrentUser {
        id: user.id.to_string(),
        username: user.username,
    })
}

#[post("/api/auth/login", state: axum::Extension<crate::state::AppState>, session: tower_sessions::Session)]
pub async fn login(username: String, password: String) -> ServerFnResult<CurrentUser> {
    let user = crate::users::find_by_username(&state.pool, username.trim())
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;
    // Same error for unknown user and wrong password — don't leak which.
    let user = user.ok_or_else(|| ServerFnError::new("invalid username or password"))?;
    if !verify_password(&password, &user.password_hash) {
        return Err(ServerFnError::new("invalid username or password"));
    }
    session
        .insert(SESSION_USER_KEY, user.id)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;
    Ok(CurrentUser {
        id: user.id.to_string(),
        username: user.username,
    })
}

#[post("/api/auth/logout", session: tower_sessions::Session)]
pub async fn logout() -> ServerFnResult<()> {
    session
        .flush()
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;
    Ok(())
}

#[get("/api/auth/me", state: axum::Extension<crate::state::AppState>, session: tower_sessions::Session)]
pub async fn current_user() -> ServerFnResult<Option<CurrentUser>> {
    let Ok(Some(user_id)) = session.get::<uuid::Uuid>(SESSION_USER_KEY).await else {
        return Ok(None);
    };
    let user = crate::users::find_by_id(&state.pool, user_id)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;
    Ok(user.map(|u| CurrentUser {
        id: u.id.to_string(),
        username: u.username,
    }))
}

#[cfg(all(test, feature = "server"))]
mod tests {
    #[test]
    fn password_hash_round_trip() {
        let hash = super::hash_password("hunter2").expect("hashing should work");
        assert!(super::verify_password("hunter2", &hash));
        assert!(!super::verify_password("wrong-password", &hash));
        // Salted: hashing the same password twice yields different strings.
        let hash2 = super::hash_password("hunter2").unwrap();
        assert_ne!(hash, hash2);
    }
}
```

- [ ] **Step 10: Rewrite `src/server.rs` (order server fns with named endpoints)**

```rust
//! `#[server]` functions for orders: the bridge between the Dioxus UI and the
//! Postgres order store + graphile_worker queue. Bodies run server-side only;
//! the macros generate the client-side stubs. Each fn has an explicit HTTP
//! endpoint so the API is stable and curl-able.

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct OrderInput {
    pub item: String,
    pub amount: u32,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct OrderDto {
    pub id: String,
    pub item: String,
    pub amount: i64,
    pub status: String,
}

#[cfg(feature = "server")]
fn dto(row: crate::orders::OrderRow) -> OrderDto {
    OrderDto {
        id: row.id.to_string(),
        item: row.item,
        amount: row.amount,
        status: row.status,
    }
}

#[post("/api/orders/start", state: axum::Extension<crate::state::AppState>, session: tower_sessions::Session)]
pub async fn start_order(order: OrderInput) -> ServerFnResult<String> {
    let user_id = crate::auth::require_user_id(&session).await?;
    let row = crate::orders::insert(&state.pool, user_id, &order.item, order.amount)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;
    state
        .utils
        .add_job(
            crate::jobs::ValidateOrder { order_id: row.id },
            graphile_worker::JobSpec::default(),
        )
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;
    Ok(row.id.to_string())
}

#[get("/api/orders/list", state: axum::Extension<crate::state::AppState>, session: tower_sessions::Session)]
pub async fn list_orders() -> ServerFnResult<Vec<OrderDto>> {
    let user_id = crate::auth::require_user_id(&session).await?;
    let rows = crate::orders::list_for_user(&state.pool, user_id)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;
    Ok(rows.into_iter().map(dto).collect())
}

#[get("/api/orders/{id}", state: axum::Extension<crate::state::AppState>, session: tower_sessions::Session)]
pub async fn get_order(id: String) -> ServerFnResult<OrderDto> {
    let user_id = crate::auth::require_user_id(&session).await?;
    let order_id = id
        .parse::<uuid::Uuid>()
        .map_err(|e| ServerFnError::new(e.to_string()))?;
    let row = crate::orders::get_for_user(&state.pool, user_id, order_id)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?
        .ok_or_else(|| ServerFnError::new("order not found"))?;
    Ok(dto(row))
}
```

- [ ] **Step 11: Interim `src/app.rs` adaptation (client must compile)**

Replace the `use crate::server::...` import and everything that referenced `OrderStatusDto`/`submit_decision`/approve-reject with a minimal single-page version of the same UI (Task 2 turns this into the routed multi-page app). Full file:

```rust
#![allow(non_snake_case)]
//! Interim single-page UI (Task 2 replaces this with a routed multi-page app):
//! create an order and watch the job pipeline's status via polling.

use dioxus::prelude::*;

use crate::server::{list_orders, start_order, OrderDto, OrderInput};

/// Interval sleep for the polling loop. The loop only ever runs on the wasm
/// client (`use_future` does not run during native SSR); the non-wasm arm just
/// has to compile, so it parks forever without pulling in a native timer crate.
async fn sleep_ms(_ms: u32) {
    #[cfg(target_arch = "wasm32")]
    gloo_timers::future::TimeoutFuture::new(_ms).await;
    #[cfg(not(target_arch = "wasm32"))]
    std::future::pending::<()>().await;
}

pub fn App() -> Element {
    let mut item = use_signal(|| "Widget".to_string());
    let mut amount = use_signal(|| "10".to_string());
    let mut orders = use_signal(Vec::<OrderDto>::new);
    let mut error = use_signal(|| Option::<String>::None);

    use_future(move || async move {
        loop {
            match list_orders().await {
                Ok(list) => orders.set(list),
                Err(e) => error.set(Some(e.to_string())),
            }
            sleep_ms(1500).await;
        }
    });

    let create = move |_| async move {
        let amt = amount().trim().parse::<u32>().unwrap_or(0);
        match start_order(OrderInput {
            item: item(),
            amount: amt,
        })
        .await
        {
            Ok(_) => error.set(None),
            Err(e) => error.set(Some(e.to_string())),
        }
    };

    rsx! {
        style { {CSS} }
        main { class: "wrap",
            h1 { "Duroxus" }
            p { class: "sub", "Dioxus server functions driving a graphile_worker order pipeline." }

            section { class: "card",
                h2 { "New order" }
                div { class: "row",
                    input {
                        value: "{item}",
                        oninput: move |e| item.set(e.value()),
                        placeholder: "Item",
                    }
                    input {
                        r#type: "number",
                        value: "{amount}",
                        oninput: move |e| amount.set(e.value()),
                        placeholder: "Amount",
                    }
                    button { class: "primary", onclick: create, "Create order" }
                }
            }

            if let Some(e) = error() {
                p { class: "err", "Error: {e}" }
            }

            section { class: "card",
                h2 { "Orders" }
                if orders().is_empty() {
                    p { class: "muted", "No orders yet — create one above." }
                } else {
                    table {
                        thead {
                            tr { th { "Item" } th { "Amount" } th { "Id" } th { "Status" } }
                        }
                        tbody {
                            for o in orders() {
                                tr { key: "{o.id}",
                                    td { "{o.item}" }
                                    td { "{o.amount}" }
                                    td { class: "mono", "{o.id}" }
                                    td { span { class: status_class(&o.status), "{o.status}" } }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

pub fn status_class(status: &str) -> &'static str {
    match status {
        "fulfilled" => "pill ok",
        "failed" => "pill err",
        "queued" => "pill",
        _ => "pill wait", // validating / charging / fulfilling
    }
}

pub const CSS: &str = r#"
:root { color-scheme: light dark; }
* { box-sizing: border-box; }
body { margin: 0; }
.wrap { max-width: 820px; margin: 0 auto; padding: 2rem 1.25rem;
  font: 15px/1.5 system-ui, -apple-system, Segoe UI, Roboto, sans-serif; }
h1 { margin: 0; font-size: 1.9rem; letter-spacing: -0.02em; }
.sub { margin: .25rem 0 1.5rem; opacity: .7; }
.card { border: 1px solid color-mix(in srgb, currentColor 15%, transparent);
  border-radius: 12px; padding: 1.1rem 1.25rem; margin-bottom: 1.25rem; }
.card h2 { margin: 0 0 .8rem; font-size: 1.05rem; }
.row { display: flex; gap: .6rem; flex-wrap: wrap; }
.col { display: flex; flex-direction: column; gap: .6rem; }
input { flex: 1 1 140px; padding: .55rem .7rem; border-radius: 8px;
  border: 1px solid color-mix(in srgb, currentColor 25%, transparent);
  background: transparent; color: inherit; }
button { padding: .55rem .9rem; border-radius: 8px; border: 0; cursor: pointer;
  font-weight: 600; }
.primary { background: #4f46e5; color: #fff; }
.ghost { background: transparent; border: 1px solid
  color-mix(in srgb, currentColor 25%, transparent); color: inherit; }
table { width: 100%; border-collapse: collapse; }
th, td { text-align: left; padding: .55rem .5rem; border-bottom:
  1px solid color-mix(in srgb, currentColor 12%, transparent); vertical-align: middle; }
th { font-size: .78rem; text-transform: uppercase; letter-spacing: .05em; opacity: .6; }
.mono { font-family: ui-monospace, SFMono-Regular, Menlo, monospace; font-size: .82rem; }
.muted { opacity: .5; }
.err { color: #dc2626; }
.pill { display: inline-block; padding: .18rem .55rem; border-radius: 999px;
  font-size: .8rem; font-weight: 600;
  background: color-mix(in srgb, currentColor 12%, transparent); }
.pill.ok { background: #16a34a22; color: #16a34a; }
.pill.err { background: #dc262622; color: #dc2626; }
.pill.wait { background: #4f46e522; color: #6366f1; }
.nav { display: flex; align-items: center; justify-content: space-between;
  margin-bottom: 1.25rem; gap: .75rem; }
.nav .who { opacity: .7; font-size: .9rem; }
.narrow { max-width: 420px; }
a { color: #6366f1; }
"#;
```

- [ ] **Step 12: Verify both targets compile**

Run: `cargo check --features server && cargo check`
Expected: both pass with no errors (warnings about unused `get_order` from the client target are acceptable).

- [ ] **Step 13: Run the argon2 unit test**

Run: `cargo test --features server password_hash_round_trip`
Expected: `test auth::tests::password_hash_round_trip ... ok`

- [ ] **Step 14: Commit**

```bash
git add -A
git commit -m "feat: replace duroxide with graphile_worker pipeline + session auth backend"
```

---


### Task 2: Routed multi-page UI (login, register, orders)

**Files:**
- Rewrite: `src/app.rs` (Route enum + App shell + shared CSS/status_class)
- Create: `src/pages/mod.rs`, `src/pages/login.rs`, `src/pages/register.rs`, `src/pages/orders.rs`
- Modify: `src/main.rs` (add `mod pages;`)

**Interfaces:**
- Consumes: `auth::{login, register, logout, current_user, CurrentUser, UNAUTHENTICATED}`, `server::{start_order, list_orders, OrderInput, OrderDto}`, `app::{Route, status_class, CSS}` — exact signatures from Task 1.
- Produces: `app::Route` enum (`OrdersPage {}` at `/`, `LoginPage {}` at `/login`, `RegisterPage {}` at `/register`), page components `pages::{login::LoginPage, register::RegisterPage, orders::OrdersPage}`.

- [ ] **Step 1: Add `mod pages;` to `src/main.rs`** (unconditional, next to `mod app;`)

```rust
mod app;
mod auth;
mod pages;
mod server;
```

- [ ] **Step 2: Rewrite `src/app.rs`**

Keep `status_class` and `CSS` exactly as written in Task 1 Step 11 (they are already `pub`). Replace everything else with:

```rust
#![allow(non_snake_case)]
//! App shell: router + shared styles. Pages live in `crate::pages`.

use dioxus::prelude::*;

use crate::pages::login::LoginPage;
use crate::pages::orders::OrdersPage;
use crate::pages::register::RegisterPage;

#[derive(Routable, Clone, PartialEq)]
pub enum Route {
    #[route("/")]
    OrdersPage {},
    #[route("/login")]
    LoginPage {},
    #[route("/register")]
    RegisterPage {},
}

pub fn App() -> Element {
    rsx! {
        style { {CSS} }
        Router::<Route> {}
    }
}

// ... pub fn status_class + pub const CSS unchanged from Task 1 ...
```

- [ ] **Step 3: Create `src/pages/mod.rs`**

```rust
pub mod login;
pub mod orders;
pub mod register;
```

- [ ] **Step 4: Create `src/pages/login.rs`**

```rust
#![allow(non_snake_case)]
//! Login page: username/password -> `auth::login` -> navigate to orders.

use dioxus::prelude::*;

use crate::app::Route;
use crate::auth::login;

#[component]
pub fn LoginPage() -> Element {
    let mut username = use_signal(String::new);
    let mut password = use_signal(String::new);
    let mut error = use_signal(|| Option::<String>::None);
    let nav = use_navigator();

    let submit = move |_| async move {
        match login(username(), password()).await {
            Ok(_) => {
                nav.push(Route::OrdersPage {});
            }
            Err(e) => error.set(Some(e.to_string())),
        }
    };

    rsx! {
        main { class: "wrap narrow",
            h1 { "Sign in" }
            p { class: "sub", "Duroxus order pipeline demo" }
            section { class: "card",
                div { class: "col",
                    input {
                        value: "{username}",
                        oninput: move |e| username.set(e.value()),
                        placeholder: "Username",
                    }
                    input {
                        r#type: "password",
                        value: "{password}",
                        oninput: move |e| password.set(e.value()),
                        placeholder: "Password",
                    }
                    button { class: "primary", onclick: submit, "Sign in" }
                }
                if let Some(e) = error() {
                    p { class: "err", "{e}" }
                }
                p { class: "muted",
                    "No account? "
                    Link { to: Route::RegisterPage {}, "Register" }
                }
            }
        }
    }
}
```

- [ ] **Step 5: Create `src/pages/register.rs`**

```rust
#![allow(non_snake_case)]
//! Register page: username/password -> `auth::register` (auto-login) ->
//! navigate to orders.

use dioxus::prelude::*;

use crate::app::Route;
use crate::auth::register;

#[component]
pub fn RegisterPage() -> Element {
    let mut username = use_signal(String::new);
    let mut password = use_signal(String::new);
    let mut error = use_signal(|| Option::<String>::None);
    let nav = use_navigator();

    let submit = move |_| async move {
        match register(username(), password()).await {
            Ok(_) => {
                nav.push(Route::OrdersPage {});
            }
            Err(e) => error.set(Some(e.to_string())),
        }
    };

    rsx! {
        main { class: "wrap narrow",
            h1 { "Create account" }
            p { class: "sub", "Duroxus order pipeline demo" }
            section { class: "card",
                div { class: "col",
                    input {
                        value: "{username}",
                        oninput: move |e| username.set(e.value()),
                        placeholder: "Username",
                    }
                    input {
                        r#type: "password",
                        value: "{password}",
                        oninput: move |e| password.set(e.value()),
                        placeholder: "Password (min 8 chars)",
                    }
                    button { class: "primary", onclick: submit, "Register" }
                }
                if let Some(e) = error() {
                    p { class: "err", "{e}" }
                }
                p { class: "muted",
                    "Already have an account? "
                    Link { to: Route::LoginPage {}, "Sign in" }
                }
            }
        }
    }
}
```

- [ ] **Step 6: Create `src/pages/orders.rs`**

The orders UI from Task 1's interim `app.rs`, now as a component with the auth guard, header, and logout. The client-side guard is UX only — the server fns are the enforcement.

```rust
#![allow(non_snake_case)]
//! Protected orders page: create an order, watch the graphile_worker pipeline
//! advance its status via polling. Redirects to /login when unauthenticated
//! (client-side UX only — server fns enforce auth themselves).

use dioxus::prelude::*;

use crate::app::{status_class, Route};
use crate::auth::{current_user, logout, CurrentUser, UNAUTHENTICATED};
use crate::server::{list_orders, start_order, OrderDto, OrderInput};

/// Interval sleep for the polling loop. The loop only ever runs on the wasm
/// client (`use_future` does not run during native SSR); the non-wasm arm just
/// has to compile, so it parks forever without pulling in a native timer crate.
async fn sleep_ms(_ms: u32) {
    #[cfg(target_arch = "wasm32")]
    gloo_timers::future::TimeoutFuture::new(_ms).await;
    #[cfg(not(target_arch = "wasm32"))]
    std::future::pending::<()>().await;
}

#[component]
pub fn OrdersPage() -> Element {
    let mut item = use_signal(|| "Widget".to_string());
    let mut amount = use_signal(|| "10".to_string());
    let mut orders = use_signal(Vec::<OrderDto>::new);
    let mut error = use_signal(|| Option::<String>::None);
    let mut user = use_signal(|| Option::<CurrentUser>::None);
    let nav = use_navigator();

    // Client-side guard + identity for the header.
    use_future(move || async move {
        match current_user().await {
            Ok(Some(u)) => user.set(Some(u)),
            Ok(None) => {
                nav.push(Route::LoginPage {});
            }
            Err(e) => error.set(Some(e.to_string())),
        }
    });

    // Poll the order list roughly every 1.5s so status transitions show live.
    use_future(move || async move {
        loop {
            match list_orders().await {
                Ok(list) => {
                    orders.set(list);
                    error.set(None);
                }
                Err(e) => {
                    let msg = e.to_string();
                    if msg.contains(UNAUTHENTICATED) {
                        nav.push(Route::LoginPage {});
                    } else {
                        error.set(Some(msg));
                    }
                }
            }
            sleep_ms(1500).await;
        }
    });

    let create = move |_| async move {
        let amt = amount().trim().parse::<u32>().unwrap_or(0);
        match start_order(OrderInput {
            item: item(),
            amount: amt,
        })
        .await
        {
            Ok(_) => error.set(None),
            Err(e) => error.set(Some(e.to_string())),
        }
    };

    let sign_out = move |_| async move {
        let _ = logout().await;
        nav.push(Route::LoginPage {});
    };

    rsx! {
        main { class: "wrap",
            header { class: "nav",
                div {
                    h1 { "Duroxus" }
                    p { class: "sub", "Dioxus server functions driving a graphile_worker order pipeline." }
                }
                div { class: "row",
                    if let Some(u) = user() {
                        span { class: "who", "Signed in as {u.username}" }
                    }
                    button { class: "ghost", onclick: sign_out, "Sign out" }
                }
            }

            section { class: "card",
                h2 { "New order" }
                div { class: "row",
                    input {
                        value: "{item}",
                        oninput: move |e| item.set(e.value()),
                        placeholder: "Item",
                    }
                    input {
                        r#type: "number",
                        value: "{amount}",
                        oninput: move |e| amount.set(e.value()),
                        placeholder: "Amount",
                    }
                    button { class: "primary", onclick: create, "Create order" }
                }
            }

            if let Some(e) = error() {
                p { class: "err", "Error: {e}" }
            }

            section { class: "card",
                h2 { "Orders" }
                if orders().is_empty() {
                    p { class: "muted", "No orders yet — create one above." }
                } else {
                    table {
                        thead {
                            tr { th { "Item" } th { "Amount" } th { "Id" } th { "Status" } }
                        }
                        tbody {
                            for o in orders() {
                                tr { key: "{o.id}",
                                    td { "{o.item}" }
                                    td { "{o.amount}" }
                                    td { class: "mono", "{o.id}" }
                                    td { span { class: status_class(&o.status), "{o.status}" } }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
```

- [ ] **Step 7: Verify compile + manual smoke test**

Run: `cargo check --features server && cargo check`
Expected: PASS.

Then: `docker compose up -d && dx serve` and in a browser: visiting `/` redirects to `/login`; register a user; create an order; watch status walk `queued → validating → charging → fulfilling → fulfilled` (~4s); sign out returns to `/login`; signing back in shows the same orders.

- [ ] **Step 8: Commit**

```bash
git add -A
git commit -m "feat: routed UI with login/register and protected orders page"
```

---

### Task 3: End-to-end integration test (job pipeline against Postgres)

**Files:**
- Modify: `src/jobs.rs` (append `#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: `AppState::new() -> (AppState, Worker)`, `users::insert`, `orders::{insert, get_for_user, list_for_user}`, `jobs::ValidateOrder`, `auth::hash_password`.
- Produces: nothing new — test only.

Constraint carried over from the old repo: a sqlx pool is bound to the tokio runtime that created it, so keep this a SINGLE `#[tokio::test]` that owns its own `AppState`; don't share pools across test fns.

- [ ] **Step 1: Append the test to `src/jobs.rs`**

```rust
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
        let listed = crate::orders::list_for_user(&state.pool, user.id).await.unwrap();
        assert!(listed
            .iter()
            .any(|o| o.id == row.id && o.status == "fulfilled"));

        worker_handle.abort();
    }
}
```

- [ ] **Step 2: Run the full test suite**

Run: `docker compose up -d && cargo test --features server`
Expected: `password_hash_round_trip ... ok` and `order_pipeline_runs_to_fulfilled ... ok` (the pipeline takes ~4–6s).

- [ ] **Step 3: Commit**

```bash
git add src/jobs.rs
git commit -m "test: e2e order pipeline against Postgres"
```

---

### Task 4: Dockerfile + README rewrite (deployment research)

**Files:**
- Create: `Dockerfile`, `.dockerignore`
- Rewrite: `README.md`

**Interfaces:**
- Consumes: everything above (documents it). Server binary reads `DATABASE_URL`, `PORT`, `IP` env vars (Dioxus 0.7 serve convention).
- Produces: nothing code-facing.

- [ ] **Step 1: Create `Dockerfile`**

```dockerfile
# Build stage: dioxus-cli builds the wasm client + server binary in one bundle.
FROM rust:1 AS builder
RUN cargo install cargo-binstall --locked \
    && cargo binstall dioxus-cli --no-confirm
WORKDIR /app
COPY . .
RUN dx bundle --platform web --release

# Runtime stage: just the server binary + static assets.
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/dx/duroxus/release/web/ /usr/local/app/
ENV PORT=8080
ENV IP=0.0.0.0
EXPOSE 8080
WORKDIR /usr/local/app
CMD ["/usr/local/app/server"]
```

Create `.dockerignore`:

```
target
.git
.env
docs
```

Verify the output path: after a local `dx bundle --platform web --release`, confirm `target/dx/duroxus/release/web/server` exists (adjust the COPY path to what dx actually produced if the layout differs).

- [ ] **Step 2: Verify the container builds and runs**

```bash
docker build -t duroxus .
docker run --rm -e DATABASE_URL=postgres://duroxide:duroxide@host.docker.internal:5432/duroxide -p 8080:8080 duroxus
```

Expected: server starts; `curl -s localhost:8080/login` returns HTML. (Postgres from docker-compose must be up.)

- [ ] **Step 3: Rewrite `README.md`**

Structure (write real prose, not this outline — content notes per section below):

```markdown
# Duroxus — Dioxus + graphile_worker template

One-paragraph pitch: full-stack Dioxus app; orders run through a Postgres-backed
graphile_worker job pipeline (validate → charge → fulfill) with live status in
the UI; session-based auth (register/login) with a protected orders page; server
functions exposed on stable, curl-able HTTP endpoints. Everything — app data,
job queue, sessions — lives in one Postgres.

## Architecture
- Single binary: Dioxus fullstack server + embedded graphile_worker worker
  (spawned as a background tokio task in `main.rs`).
- src/jobs.rs — the three chained TaskHandlers driving orders.status
- src/auth.rs, src/users.rs — session auth (tower-sessions Postgres store, argon2)
- src/server.rs, src/orders.rs — order server fns + store
- src/app.rs, src/pages/ — router with /login, /register, and protected /
- migrations/ — sqlx migrations (users, orders); graphile_worker migrates its
  own schema; tower-sessions migrates its session table.

## Run it
    docker compose up -d
    cp .env.example .env
    dx serve
(Schema changed since the duroxide version: `docker compose down -v` first if
you have an old volume.)

## HTTP API
Table of endpoints (method, path, auth?):
POST /api/auth/register {username, password} — no
POST /api/auth/login {username, password} — no
POST /api/auth/logout — yes
GET  /api/auth/me — session-optional
POST /api/orders/start {order: {item, amount}} — yes
GET  /api/orders/list — yes
GET  /api/orders/{id} — yes

curl example (cookie jar = session):
    curl -c /tmp/jar -H 'content-type: application/json' \
      -d '{"username":"demo","password":"password123"}' \
      localhost:8080/api/auth/register
    curl -b /tmp/jar localhost:8080/api/orders/list
(Verify the actual request body shape against a running server before
committing — the server fn macro defines whether args are flattened.)

## Tests
    docker compose up -d
    cargo test --features server

## Deployment
### The build artifact
`dx bundle --platform web --release` → self-contained server binary + static
assets under `target/dx/duroxus/release/web/`. The provided Dockerfile does
this in a two-stage build. Configuration via env: DATABASE_URL, PORT, IP.

### Option 1: VPS (Hetzner Cloud / DigitalOcean)
- Cheapest always-on option: Hetzner CX22 (~€4/mo) or DO Basic Droplet ($6/mo)
  comfortably runs this app + Postgres.
- docker-compose route: extend the repo's compose file with an `app:` service
  built from the Dockerfile (show the ~10-line snippet: build: ., ports
  8080:8080, DATABASE_URL pointing at the postgres service, depends_on with
  the healthcheck).
- systemd route: build the bundle, copy binary+assets to the box, a unit file
  with Environment=DATABASE_URL/PORT/IP and Restart=always; put Caddy or
  nginx in front for TLS.
- Database: co-located Postgres container/package is fine at this scale;
  managed (DO Managed Postgres from ~$15/mo) buys backups + upgrades —
  always-on, so fully compatible with the worker.

### Option 2: Railway
- Dockerfile is auto-detected; add a Postgres service in the same project and
  set DATABASE_URL to the reference variable ${{Postgres.DATABASE_URL}}.
- Railway injects PORT — the server already reads it; IP=0.0.0.0 is set in the
  Dockerfile.
- Turn OFF "app sleeping"/serverless for the service: the embedded worker must
  stay resident to poll the queue. Railway Postgres is a plain always-on
  instance — no scale-to-zero surprises.

### Option 3: Fly.io
- `fly launch` picks up the Dockerfile; set internal_port = 8080 in fly.toml.
- Keep `min_machines_running = 1` (and auto_stop_machines off) — Fly's default
  stop-when-idle would suspend the embedded worker and jobs would sit queued.
- Database: Fly Postgres (unmanaged) or a managed provider (e.g. Supabase,
  Crunchy). Attach sets DATABASE_URL.

### Choosing the database (and the Neon caveat)
- graphile_worker keeps a LISTEN connection open and polls (default 1s). Two
  consequences: (1) serverless scale-to-zero Postgres (Neon autosuspend) never
  actually idles — the LISTEN session keeps compute awake, so you pay always-on
  prices with extra cold-start risk; (2) transaction-mode poolers (PgBouncer,
  Neon's pooled connection string) don't support LISTEN/NOTIFY session state —
  the worker must use a direct connection.
- So: prefer a plain always-on Postgres (VPS container, DO/Railway/Fly
  Postgres). If you must use Neon, use the direct (non-pooled) URL and expect
  compute to never suspend.
- Keep `worker concurrency × pool max_connections` under the provider's
  connection limit (this template: concurrency 2, pool 10).

### Production notes
- Set the session cookie `with_secure(true)` behind TLS (see main.rs).
- The worker is embedded for simplicity; to scale it independently, move
  `worker.run()` into a second binary that reuses `src/jobs.rs` and deploy it
  as a separate process sharing the same DATABASE_URL.
```

- [ ] **Step 4: Verify the curl examples against a running server**

Run `dx serve`, then execute the README's register + list curls; paste the real request/response shapes into the README (fix the body nesting for `start_order` if the macro expects `{"order": {...}}` vs flattened fields).

- [ ] **Step 5: Commit**

```bash
git add Dockerfile .dockerignore README.md
git commit -m "docs: README rewrite with deployment guide; add Dockerfile"
```

---

## Final verification (after all tasks)

- [ ] `cargo check && cargo check --features server` — both clean
- [ ] `cargo test --features server` with Postgres up — all green
- [ ] `grep -ri duroxide src/ Cargo.toml` — only hits allowed are the docker-compose credentials/db name and README history notes
- [ ] Manual: full browser flow (register → order → watch pipeline → logout → login)
- [ ] `docker build -t duroxus .` succeeds
