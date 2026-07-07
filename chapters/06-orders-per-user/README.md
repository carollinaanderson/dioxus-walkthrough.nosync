# Chapter 6 — Orders per user

## What you'll learn

Turning "you must be logged in" into "you can only see *your* stuff": a
foreign key, a scoped query, and the difference between authentication and
authorization.

## Run it

```bash
docker compose up -d          # Postgres 16 on localhost:5436
cp .env.example .env
dx serve
```

Register two different accounts (two browser profiles, or one normal + one
incognito window) and create orders in each. Every account only ever sees
its own orders now.

## How it works

- **`migrations/0003_add_user_id_to_orders.sql`** adds `user_id` as a
  nullable column first, then makes it `NOT NULL` in a second statement.
  That two-step is a real-world pattern: on a table with existing rows,
  adding a `NOT NULL` column in one step fails (existing rows have no value
  to put there) — you add it nullable, backfill, *then* tighten it. Our
  table happens to be empty, but the migration is written the way you'd
  write it against a live database.
- **`src/orders.rs`**: every function now takes a `user_id: Uuid` and puts
  it in the query — `insert` writes it, `list_for_user`/`get_for_user` add
  `WHERE user_id = $1`. There is no code path that returns another user's
  order, because the SQL itself won't produce one.
- **`src/server.rs`**: `require_user_id`'s return value is no longer
  discarded — it's the `user_id` passed into every store call.
  **This is the authentication → authorization line**: chapter 5 answered
  "is there *a* valid user?"; this chapter answers "which rows belong to
  *that* user?".
- **`get_order`** is a new endpoint, `GET /api/orders/{id}`, that looks up
  one order by id — but still scoped by `user_id`, so requesting someone
  else's order id returns "order not found", not their data.

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

2. **Add `graphile_worker`** as a server-only dependency:

   ```toml
   # =0.13.1: last version on sqlx 0.8; later versions moved to sqlx 0.9,
   # which tower-sessions-sqlx-store (sqlx 0.8) can't share a pool with.
   graphile_worker = { version = "=0.13.1", optional = true }
   ```

   and `"dep:graphile_worker"` to the `server` feature list. It manages its
   own Postgres schema (creatively named `graphile_worker`) inside the same
   database, migrated automatically when you initialize it in the next
   step — no new `.sql` migration file of your own needed. This is the
   whole appeal of a Postgres-backed job queue: no separate broker process
   (no Redis, no RabbitMQ) to run alongside your app.

3. **Add `orders::set_status`** to `orders.rs` — the one write jobs will
   make:

   ```rust
   pub async fn set_status(pool: &PgPool, id: Uuid, status: &str) -> Result<(), sqlx::Error> {
       sqlx::query("UPDATE orders SET status = $2 WHERE id = $1")
           .bind(id)
           .bind(status)
           .execute(pool)
           .await?;
       Ok(())
   }
   ```

4. **Write `src/jobs.rs`**. A `TaskHandler` is just a struct (which
   `graphile_worker` serializes into the queue) plus an `async fn run` that
   does the work:

   ```rust
   use graphile_worker::{IntoTaskHandlerResult, JobSpec, TaskHandler, WorkerContext, WorkerContextExt};
   use serde::{Deserialize, Serialize};
   use uuid::Uuid;

   const STEP_DELAY: std::time::Duration = std::time::Duration::from_millis(1200);

   async fn set_stage(ctx: &WorkerContext, order_id: Uuid, stage: &str) -> Result<(), String> {
       crate::orders::set_status(ctx.pg_pool(), order_id, stage)
           .await
           .map_err(|e| e.to_string())
   }

   #[derive(Serialize, Deserialize)]
   pub struct ValidateOrder {
       pub order_id: Uuid,
   }

   impl TaskHandler for ValidateOrder {
       const IDENTIFIER: &'static str = "validate_order";

       async fn run(self, ctx: WorkerContext) -> impl IntoTaskHandlerResult {
           set_stage(&ctx, self.order_id, "validating").await?;
           tokio::time::sleep(STEP_DELAY).await;
           ctx.add_job(ChargePayment { order_id: self.order_id }, JobSpec::default())
               .await
               .map(|_| ())
               .map_err(|e| e.to_string())
       }
   }
   ```

   `const IDENTIFIER` is the string `graphile_worker` stores in Postgres to
   know which handler to run for a queued job — it has to be unique across
   all your job types. `ctx.pg_pool()` hands back the same pool your
   `AppState` uses — jobs and server fns share one connection pool, one
   database. `ctx.add_job(next_job, JobSpec::default())` is how a job
   enqueues the *next* job in the chain — `ChargePayment` here, which itself
   enqueues `FulfillOrder`, which (being last) calls `set_stage(...,
   "fulfilled")` instead of enqueueing anything further. Write
   `ChargePayment` and `FulfillOrder` the same way, following the chain.

   Wrap each handler's body so a failure marks the order `"failed"` instead
   of leaving it stuck mid-pipeline (see the reference `or_fail` helper in
   this chapter's `jobs.rs`) — without that, an error partway through would
   just look like the order silently stopped updating, with no indication
   anything went wrong.

5. **Extend `AppState` to own the worker.** `state.rs`'s `AppState::new`
   needs to register every handler type, start the worker, and hand back a
   way to enqueue jobs:

   ```rust
   use graphile_worker::{WorkerOptions, WorkerUtils};

   #[derive(Clone)]
   pub struct AppState {
       pub pool: PgPool,
       pub worker: WorkerUtils,
   }

   // inside AppState::new, after running your own sqlx migrations:
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
   let worker_utils = worker.create_utils();
   let worker_handle = tokio::spawn(async move { worker.run().await });
   ```

   `.define_job::<T>()` registers each `TaskHandler` type so the worker
   knows how to deserialize and dispatch jobs with that `IDENTIFIER`.
   `.init()` is what actually runs graphile_worker's own schema migration
   against your database. `worker.create_utils()` gives you a cheap,
   cloneable `WorkerUtils` handle (store it in `AppState`, next to `pool`)
   that server functions use to enqueue jobs — the `worker` value itself,
   which owns the polling loop, gets moved into `tokio::spawn(...)` and
   left running in the background for the lifetime of the process.

6. **Kick off the pipeline in `start_order`:**

   ```rust
   state
       .worker
       .add_job(crate::jobs::ValidateOrder { order_id: row.id }, graphile_worker::JobSpec::default())
       .await
       .map_err(|e| ServerFnError::new(e.to_string()))?;
   ```

   right after `orders::insert`. This is the only place any job gets
   enqueued from outside the pipeline itself — everything after this is
   jobs enqueueing each other.

7. **Poll from the UI.** Replace the manual "Refresh" button in
   `pages/orders.rs` with a loop:

   ```rust
   async fn sleep_ms(_ms: u32) {
       #[cfg(target_arch = "wasm32")]
       gloo_timers::future::TimeoutFuture::new(_ms).await;
       #[cfg(not(target_arch = "wasm32"))]
       std::future::pending::<()>().await;
   }

   use_future(move || async move {
       loop {
           match list_orders().await {
               Ok(list) => { orders.set(list); error.set(None); }
               Err(e) => { /* same UNAUTHENTICATED check as before */ }
           }
           sleep_ms(1500).await;
       }
   });
   ```

   Why the odd `#[cfg]` split inside `sleep_ms`? This function only ever
   *runs* in the browser (native SSR doesn't execute `use_future` bodies),
   but it still has to *compile* for both targets. WASM has no `tokio`
   runtime to sleep on, so the wasm arm uses `gloo-timers`; the non-wasm arm
   just needs to type-check, so it parks forever on `pending()` rather than
   pulling in a native timer dependency it will never use.

   Then add a status → CSS class mapping in `app.rs`:

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

   and use it in the table: `span { class: status_class(&o.status), "{o.status}" }`.
   The `.pill`/`.pill.ok`/`.pill.err`/`.pill.wait` CSS rules were already
   sitting unused in your stylesheet since chapter 1 — this is the chapter
   that finally uses them.

8. **Add a `Dockerfile`** for deployment. See this chapter's for a working
   multi-stage build (build with `dx bundle`, ship just the resulting
   binary + assets in a slim runtime image).

Watch an order you create walk `queued → validating → charging → fulfilling
→ fulfilled` in the UI without touching refresh.

## Check your work

[chapters/07-background-jobs](../07-background-jobs) has the full working
version — the same app documented in the root README, including an e2e test
that runs the pipeline against real Postgres.

**Next:** [Chapter 7 — Background jobs](../07-background-jobs/README.md)
