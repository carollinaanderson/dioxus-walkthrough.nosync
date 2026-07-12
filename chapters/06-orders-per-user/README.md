# Chapter 6 — Orders per user

## What you'll learn

Turning "you must be logged in" into "you can only see *your* stuff": a
scoped query keyed by the Clerk user id, and the difference between
authentication and authorization.

## Run it

```bash
docker compose up -d          # Postgres 16 on localhost:5436
cp .env.example .env          # paste your pk_test_… / sk_test_… keys
dx serve
```

Register two different accounts (two browser profiles, or one normal + one
incognito window) and create orders in each. Every account only ever sees
its own orders now.

## How it works

- **`migrations/0002_add_user_id_to_orders.sql`** adds `user_id` as a
  nullable column first, then makes it `NOT NULL` in a second statement.
  That two-step is a real-world pattern: on a table with existing rows,
  adding a `NOT NULL` column in one step fails (existing rows have no value
  to put there) — you add it nullable, backfill, *then* tighten it. Our
  table happens to be empty, but the migration is written the way you'd
  write it against a live database. The column type is `TEXT`, not `UUID`,
  and it is a plain column, **not** a foreign key: Clerk user ids are
  strings like `user_2abc…`, and there is no local `users` table to
  reference because accounts live in Clerk.
- **`src/orders.rs`**: every function takes a `user_id: &str` and puts
  it in the query — `insert` writes it, `list_for_user`/`get_for_user` add
  `WHERE user_id = $1`. There is no code path that returns another user's
  order, because the SQL itself won't produce one.
- **`src/server.rs`**: `current_auth()`'s return value is no longer
  discarded — it's the Clerk `user_id` passed into every store call.
  **This is the authentication → authorization line**: chapter 5 answered
  "is there *a* valid user?"; this chapter answers "which rows belong to
  *that* user?".
- **`get_order`** is a new endpoint, `GET /api/orders/{id}`, that looks up
  one order by id — but still scoped by `user_id`, so requesting someone
  else's order id returns "order not found", not their data.
- **`build.rs`** loads `.env` at build time with
  [`dotenvy`](https://crates.io/crates/dotenvy) so
  `env!("CLERK_PUBLISHABLE_KEY")` resolves from your `.env` without a manual
  `export` (see [chapter 4](../04-user-accounts/README.md) for the full
  explanation).

## Your turn: get to chapter 7

The last chapter replaces the frozen `'queued'` status with a real
background pipeline: `graphile_worker` chains three jobs
(`validate_order → charge_payment → fulfill_order`), each advancing
`orders.status`, and the UI polls to show it happening live.

1. **Copy this chapter as your working copy:**

   ```bash
   cp -r ../06-orders-per-user ../my-07-background-jobs
   cd ../my-07-background-jobs
   ```

2. **Add the job-queue dependencies.** `graphile_worker` is server-only and
   version-pinned — add it and its two subcrates under `[dependencies]`, one
   line at a time:

   ```toml
   # =0.13.1: last version on sqlx 0.8; later versions moved to sqlx 0.9.
   # graphile_worker shares one PgPool with your own order queries here, so
   # both must agree on sqlx's major version.
   graphile_worker = { version = "=0.13.1", optional = true }         # <-- add this
   graphile_worker_ctx = { version = "=0.5.2", optional = true }      # <-- add this
   graphile_worker_database = { version = "=0.1.3", optional = true } # <-- add this
   ```

   The polling UI (step 7) needs a browser timer, so add `gloo-timers` in a
   client-only target block — it compiles into the WASM bundle, never the
   server binary:

   ```toml
   [target.'cfg(target_arch = "wasm32")'.dependencies] # <-- add this section
   gloo-timers = { version = "0.3", features = ["futures"] }
   ```

   Then extend the `server` feature with the three job crates:

   ```toml
   server = [
       "dioxus/server", "dep:axum", "dep:tokio", "dep:sqlx", "dep:uuid", "dep:dotenvy",
       "dep:graphile_worker",          # <-- add this
       "dep:graphile_worker_ctx",      # <-- add this
       "dep:graphile_worker_database", # <-- add this
       "dioxus-clerk/server",
   ]
   ```

   `graphile_worker` manages its own Postgres schema (named `graphile_worker`)
   inside the same database, migrated automatically when you initialize it in
   step 5 — no `.sql` migration of your own needed. That's the appeal of a
   Postgres-backed queue: no separate broker process (no Redis, no RabbitMQ) to
   run alongside your app.

3. **Add `orders::set_status`** to `orders.rs` — the one write the jobs will
   make. Empty shell first:

   ```rust
   pub async fn set_status(pool: &PgPool, id: Uuid, status: &str) -> Result<(), sqlx::Error> {
       // fill in next
   }
   ```

   then the update — it uses `.execute` (no rows returned), unlike the
   `query_as` reads:

   ```rust
   pub async fn set_status(pool: &PgPool, id: Uuid, status: &str) -> Result<(), sqlx::Error> {
       sqlx::query("UPDATE orders SET status = $2 WHERE id = $1")
           .bind(id)
           .bind(status)
           .execute(pool) // <-- runs the statement, discards any rows
           .await?;
       Ok(())
   }
   ```

4. **Write `src/jobs.rs`.** A `TaskHandler` is a struct (which
   `graphile_worker` serializes into the queue) plus an `async fn run` that
   does the work. Start with the imports and the per-step delay that lets the
   UI visibly walk each stage:

   ```rust
   use std::time::Duration;

   use graphile_worker::{
       IntoTaskHandlerResult, JobSpec, TaskHandler, WorkerContext, WorkerContextExt,
   };
   use serde::{Deserialize, Serialize};
   use uuid::Uuid;

   const STEP_DELAY: Duration = Duration::from_millis(1200);
   ```

   All three handlers share the same moves — stamp a stage, enqueue the next
   job, mark the order failed on error — so factor those into helpers first.
   `set_stage` writes one status; add it as a shell:

   ```rust
   async fn set_stage(ctx: &WorkerContext, order_id: Uuid, stage: &str) -> Result<(), String> {
       // fill in next
   }
   ```

   then fill it — `ctx.pg_pool()` hands back the *same* pool your `AppState`
   uses, so jobs and server fns share one connection pool and one database:

   ```rust
   async fn set_stage(ctx: &WorkerContext, order_id: Uuid, stage: &str) -> Result<(), String> {
       crate::orders::set_status(ctx.pg_pool(), order_id, stage)
           .await
           .map_err(|e| e.to_string())
   }
   ```

   `enqueue` adds the next job to the queue — shell first:

   ```rust
   async fn enqueue<T: TaskHandler + 'static>(ctx: &WorkerContext, job: T) -> Result<(), String> {
       // fill in next
   }
   ```

   then the body:

   ```rust
   async fn enqueue<T: TaskHandler + 'static>(ctx: &WorkerContext, job: T) -> Result<(), String> {
       ctx.add_job(job, JobSpec::default())
           .await
           .map(|_| ())
           .map_err(|e| e.to_string())
   }
   ```

   `or_fail` marks the order `"failed"` if a step errored, then passes the
   error through — without it, a mid-pipeline failure would just look like the
   order silently stopping:

   ```rust
   async fn or_fail(ctx: &WorkerContext, order_id: Uuid, res: Result<(), String>) -> Result<(), String> {
       if res.is_err() {
           let _ = crate::orders::set_status(ctx.pg_pool(), order_id, "failed").await;
       }
       res
   }
   ```

   Now the first handler. Add the struct and an *empty* `impl` so the shape is
   clear before the body:

   ```rust
   #[derive(Serialize, Deserialize)]
   pub struct ValidateOrder {
       pub order_id: Uuid,
   }

   impl TaskHandler for ValidateOrder {
       const IDENTIFIER: &'static str = "validate_order"; // unique per job type
       async fn run(self, ctx: WorkerContext) -> impl IntoTaskHandlerResult {
           // fill in next
       }
   }
   ```

   `const IDENTIFIER` is the string `graphile_worker` stores in Postgres to
   know which handler to run for a queued job — it must be unique across your
   job types. Fill the body: run the three steps inside one `async` block (so a
   single `?` short-circuits them), then pass the whole result through
   `or_fail`:

   ```rust
       async fn run(self, ctx: WorkerContext) -> impl IntoTaskHandlerResult {
           let step = async {
               set_stage(&ctx, self.order_id, "validating").await?;          // <-- stamp the stage
               tokio::time::sleep(STEP_DELAY).await;                         // <-- simulate work
               enqueue(&ctx, ChargePayment { order_id: self.order_id }).await // <-- hand off to the next job
           }
           .await;
           or_fail(&ctx, self.order_id, step).await // <-- mark 'failed' on any error
       }
   ```

   `ChargePayment` is the same shape — only the identifier, the stage string,
   and the next job differ:

   ```rust
   #[derive(Serialize, Deserialize)]
   pub struct ChargePayment {
       pub order_id: Uuid,
   }

   impl TaskHandler for ChargePayment {
       const IDENTIFIER: &'static str = "charge_payment"; // <-- differs
       async fn run(self, ctx: WorkerContext) -> impl IntoTaskHandlerResult {
           let step = async {
               set_stage(&ctx, self.order_id, "charging").await?;            // <-- differs
               tokio::time::sleep(STEP_DELAY).await;
               enqueue(&ctx, FulfillOrder { order_id: self.order_id }).await // <-- differs
           }
           .await;
           or_fail(&ctx, self.order_id, step).await
       }
   }
   ```

   `FulfillOrder` is the last link, so instead of enqueueing it just sets the
   terminal `"fulfilled"` status:

   ```rust
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
               set_stage(&ctx, self.order_id, "fulfilled").await // <-- terminal: no enqueue
           }
           .await;
           or_fail(&ctx, self.order_id, step).await
       }
   }
   ```

   (The reference `jobs.rs` also carries a `#[cfg(test)]` end-to-end test that
   drives an order to `fulfilled` against real Postgres — see the [Tests
   section](../07-background-jobs/README.md#tests) of chapter 7.)

5. **Extend `AppState` to own the worker.** In `state.rs`, add the imports and
   a `worker` field to the struct:

   ```rust
   use graphile_worker::runner::WorkerRuntimeError; // <-- add this
   use graphile_worker::{WorkerOptions, WorkerUtils}; // <-- add this
   use tokio::task::JoinHandle;                       // <-- add this

   #[derive(Clone)]
   pub struct AppState {
       pub pool: PgPool,
       pub worker: WorkerUtils, // <-- add this
   }
   ```

   Change `new`'s signature so it also hands back the worker's background task
   handle:

   ```rust
   pub async fn new() -> (Self, JoinHandle<Result<(), WorkerRuntimeError>>) { // <-- was: -> Self
   ```

   Inside `new`, after running your own sqlx migrations, build and start the
   worker — registering each handler type:

   ```rust
       let worker = WorkerOptions::default()
           .pg_pool(pool.clone())                      // <-- the same pool as your queries
           .schema("graphile_worker")
           .concurrency(2)
           .define_job::<crate::jobs::ValidateOrder>() // <-- register each handler
           .define_job::<crate::jobs::ChargePayment>()
           .define_job::<crate::jobs::FulfillOrder>()
           .init()                                     // <-- runs graphile_worker's own migration
           .await
           .expect("failed to initialize graphile_worker");
       let worker_utils = worker.create_utils();                           // <-- cheap enqueue handle
       let worker_handle = tokio::spawn(async move { worker.run().await }); // <-- polling loop, backgrounded
   ```

   Then return the tuple instead of a bare `Self`:

   ```rust
       (Self { pool, worker: worker_utils }, worker_handle) // <-- was: Self { pool }
   ```

   `.define_job::<T>()` registers each `TaskHandler` so the worker can
   deserialize and dispatch jobs with that `IDENTIFIER`; `.init()` runs
   graphile_worker's own schema migration; `create_utils()` gives a cheap,
   cloneable handle that server fns use to enqueue. Because `new` now returns a
   tuple, update `main.rs` — declare the new module and destructure the result:

   ```rust
   #[cfg(feature = "server")]
   mod jobs; // <-- add this, next to `mod orders;` / `mod state;`
   ```

   ```rust
       let (state, _) = state::AppState::new().await; // <-- was: let state = state::AppState::new().await;
   ```

   The `_` drops the worker `JoinHandle`; the spawned task keeps running for
   the life of the process regardless.

6. **Kick off the pipeline in `start_order`.** In `server.rs`, enqueue the
   first job right after inserting the row and before returning its id:

   ```rust
   pub async fn start_order(order: OrderInput) -> ServerFnResult<String> {
       let user_id = dioxus_clerk::server::current_auth()?;
       let row = crate::orders::insert(&state.pool, &user_id, &order.item, order.amount)
           .await
           .map_err(ServerFnError::new)?;
       state                                             // <-- add this block
           .worker
           .add_job(
               crate::jobs::ValidateOrder { order_id: row.id },
               graphile_worker::JobSpec::default(),
           )
           .await
           .map_err(ServerFnError::new)?;
       Ok(row.id.to_string())
   }
   ```

   This is the only place a job is enqueued from outside the pipeline — every
   job after `ValidateOrder` is enqueued by the one before it.

7. **Poll from the UI.** In `pages/orders.rs`, the status now changes on the
   server *after* the response, so a one-shot fetch isn't enough — poll
   instead. First add a cross-target sleep helper:

   ```rust
   async fn sleep_ms(_ms: u32) {
       #[cfg(target_arch = "wasm32")]
       gloo_timers::future::TimeoutFuture::new(_ms).await;
       #[cfg(not(target_arch = "wasm32"))]
       std::future::pending::<()>().await;
   }
   ```

   Why the `#[cfg]` split? This only ever *runs* in the browser (native SSR
   doesn't execute `use_future` bodies), but it still has to *compile* for both
   targets. WASM has no `tokio` runtime to sleep on, so the wasm arm uses
   `gloo-timers`; the non-wasm arm just needs to type-check, so it parks
   forever on `pending()` rather than pulling in a native timer crate.

   Turn the one-shot loader into a polling loop — fetch, sleep ~1.5s, repeat:

   ```rust
   use_future(move || async move {
       loop {                                      // <-- was a single fetch
           match list_orders().await {
               Ok(list) => { orders.set(list); error.set(None); }
               Err(e) => error.set(Some(e.to_string())),
           }
           sleep_ms(1500).await;                   // <-- then wait and go again
       }
   });
   ```

   With the poll running, manual refresh is redundant — delete the `refresh`
   handler and its `button { onclick: refresh, "Refresh" }`, and drop the
   refetch from `create` (the loop picks up the new order on its next pass):

   ```rust
   let create = move |_| async move {
       let amt = amount().trim().parse::<u32>().unwrap_or(0);
       match start_order(OrderInput { item: item(), amount: amt }).await {
           Ok(_) => error.set(None),                 // <-- no refetch; the poll handles it
           Err(e) => error.set(Some(e.to_string())),
       }
   };
   ```

   Finally, make status visible. Add a status → CSS class mapping in `app.rs`:

   ```rust
   pub fn status_class(status: &str) -> &'static str {
       match status {
           "fulfilled" => "pill ok",
           "failed" => "pill err",
           "queued" => "pill",
           _ => "pill wait", // validating / charging / fulfilling
       }
   }
   ```

   Import it in `pages/orders.rs` (`use crate::app::status_class;`) and wrap the
   status cell in a pill:

   ```rust
   td { span { class: status_class(&o.status), "{o.status}" } } // <-- was: td { "{o.status}" }
   ```

   The `.pill`/`.pill.ok`/`.pill.err`/`.pill.wait` rules have been sitting
   unused in your stylesheet since chapter 1 — this is the chapter that finally
   uses them.

8. **Add a `Dockerfile`** for deployment. Copy this chapter's — a working
   multi-stage build: a `rust:1` stage runs `dx bundle --platform web
   --release` (forwarding `CLERK_PUBLISHABLE_KEY` as a build arg, since `env!`
   reads it at compile time), then a slim `debian:bookworm-slim` stage copies
   out just the server binary and static assets. See chapter 7's [Deploying
   section](../07-background-jobs/README.md#deploying) for the run command.

Watch an order you create walk `queued → validating → charging → fulfilling →
fulfilled` in the UI without touching refresh.

## Check your work

[chapters/07-background-jobs](../07-background-jobs) has the full working
version — the same app documented in the root README. Because accounts live
in Clerk, there's no local `users` table and no `register` server fn: the
server-side identity comes entirely from Clerk's verified session via
`current_auth()`.

**Next:** [Chapter 7 — Background jobs](../07-background-jobs/README.md)
