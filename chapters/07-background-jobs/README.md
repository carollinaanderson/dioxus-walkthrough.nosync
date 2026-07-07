# Chapter 7 — Background jobs

This is the final chapter — the same app described in the [root
README](../../README.md): Dioxus + session auth + Postgres-backed orders +
a `graphile_worker` job pipeline, all in one binary.

## What you'll learn

How to run real background work — the kind that shouldn't block an HTTP
request — using [`graphile_worker`](https://github.com/leo91000/graphile_worker_rs),
a job queue that lives entirely inside your Postgres database. You'll chain
three jobs together and watch the UI reflect their progress live.

## Run it

```bash
docker compose up -d          # Postgres 16 on localhost:5437
cp .env.example .env
dx serve                      # http://localhost:8080
```

Register, create an order, and watch its status pill walk
`queued → validating → charging → fulfilling → fulfilled` (~4 seconds) with
no manual refresh.

## How it works

- **Jobs live in Postgres.** `graphile_worker` uses the same Postgres
  database as everything else — it creates its own `graphile_worker` schema
  for the job queue, migrated on boot alongside your `sqlx` migrations.
  There's no separate broker (no Redis, no RabbitMQ) to run or deploy.
- **`src/jobs.rs`** defines three `TaskHandler`s, each just a struct holding
  an `order_id`. `ValidateOrder::run` stamps `orders.status = 'validating'`,
  sleeps briefly (standing in for real work), and enqueues `ChargePayment`.
  That enqueues `FulfillOrder`, which sets the final `'fulfilled'` status.
  Any step's error marks the order `'failed'` instead of leaving it stuck.
- **`src/state.rs`** builds a `WorkerOptions`, registers all three handler
  types with `.define_job::<T>()`, and `tokio::spawn`s the worker's run loop
  as a background task alongside the axum server — one process, two jobs.
- **`start_order`** (in `server.rs`) enqueues the first job,
  `ValidateOrder`, right after inserting the row — that's the only place the
  pipeline gets kicked off.
- **The UI polls.** `pages/orders.rs` loops: fetch the order list, sleep
  ~1.5s (via `gloo-timers`, since this only ever runs in the WASM client),
  repeat. `app.rs`'s `status_class` maps each status string to a `.pill`
  CSS class (`ok` for fulfilled, `err` for failed, `wait` for anything
  in-flight) so progress is visible at a glance.
- **The e2e test** (`src/jobs.rs`, `order_pipeline_runs_to_fulfilled`) boots
  a real `AppState` against your running Postgres, enqueues a job, and polls
  the database directly until the order reaches `'fulfilled'` — proving the
  whole chain actually runs, not just that each handler compiles.

### Version pinning note

`graphile_worker` is pinned to `=0.13.1` (and its subcrates are pinned in
`Cargo.lock`): later patches moved to sqlx 0.9, while
`tower-sessions-sqlx-store` is still on sqlx 0.8 — with mismatched sqlx
majors they can't share one `PgPool`. When the sessions store catches up,
unpin both. Avoid a blanket `cargo update` (it would float the subcrates
back onto sqlx 0.9); update specific packages instead.

## HTTP API

Server functions are declared with explicit routes
(`#[post("/api/orders/start", ...)]`), so the wire API is stable instead of
macro-hashed:

| Method | Path | Body | Auth |
|---|---|---|---|
| POST | `/api/auth/register` | `{"username", "password"}` | — |
| POST | `/api/auth/login` | `{"username", "password"}` | — |
| POST | `/api/auth/logout` | — | session |
| GET | `/api/auth/me` | — | optional |
| POST | `/api/orders/start` | `{"order": {"item", "amount"}}` | session |
| GET | `/api/orders/list` | — | session |
| GET | `/api/orders/{id}` | — | session |

The session rides a cookie, so curl works with a cookie jar:

```bash
curl -c /tmp/jar -H 'content-type: application/json' \
  -d '{"username":"demo","password":"password123"}' \
  localhost:8080/api/auth/register
# {"id":"7dd9685c-...","username":"demo"}

curl -b /tmp/jar -H 'content-type: application/json' \
  -d '{"order":{"item":"Widget","amount":10}}' \
  localhost:8080/api/orders/start
# "7db27402-..."

curl -b /tmp/jar localhost:8080/api/orders/list
# [{"id":"7db27402-...","item":"Widget","amount":10,"status":"validating"}]
```

Unauthenticated calls to protected endpoints fail with an `unauthenticated`
error, which the UI maps to a redirect to `/login`. Server functions are the
enforcement boundary — the client-side route guard is only UX.

## Tests

```bash
docker compose up -d
cargo test --features server
```

Two tests: the argon2 hash round-trip (`src/auth.rs`) and the end-to-end
pipeline test (`src/jobs.rs`) that boots the worker against real Postgres
and asserts an order reaches `fulfilled` through every intermediate stage.

## Deploying

The `Dockerfile` here is a working multi-stage build: a `rust:1` stage
installs `dioxus-cli` and runs `dx bundle --platform web --release`, then a
slim `debian:bookworm-slim` runtime stage copies out just the server binary
and static assets.

Build it with this chapter directory as the context (it's a standalone
crate, so it doesn't need the rest of the workspace):

```bash
cd chapters/07-background-jobs
docker build -t myapp .
docker run -p 8080:8080 --env DATABASE_URL=... myapp
```

In production, set `DATABASE_URL` to a real Postgres instance and put the
app behind TLS — and change `with_secure(false)` to `with_secure(true)` in
`src/main.rs`'s session layer once you're serving over https, so session
cookies are marked secure.

## You made it

You've built a full-stack Dioxus app from an empty `dx new` up through
sessions, per-user data, and a real background job pipeline — the same
shape of app you'd reach for at a startup or a side project. From here,
poking at this chapter directly (add a new job stage, add rate limiting to
`start_order`, swap the CSS for a UI library) is a good way to make it your
own.
