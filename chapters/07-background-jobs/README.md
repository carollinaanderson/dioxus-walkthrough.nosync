# Chapter 7 — Background jobs

This is the final chapter — the same app described in the [root
README](../../README.md): Dioxus + Clerk auth + Postgres-backed orders +
a `graphile_worker` job pipeline, all in one binary.

## What you'll learn

How to run real background work — the kind that shouldn't block an HTTP
request — using [`graphile_worker`](https://github.com/leo91000/graphile_worker_rs),
a job queue that lives entirely inside your Postgres database. You'll chain
three jobs together and watch the UI reflect their progress live.

## Run it

```bash
docker compose up -d          # Postgres 16 on localhost:5437
cp .env.example .env          # paste your pk_test_… / sk_test_… keys
dx serve                      # http://localhost:8080
```

Sign in through Clerk, create an order, and watch its status pill walk
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
- **Auth is Clerk.** The page is gated by `SignedOut { RedirectToSignIn }` /
  `SignedIn { … }`; the server trusts the identity that `ClerkAuthLayer`
  verified from the session cookie, read via `require_user_id()`.
- **The e2e test** (`src/jobs.rs`, `order_pipeline_runs_to_fulfilled`) boots
  a real `AppState` against your running Postgres, enqueues a job, and polls
  the database directly until the order reaches `'fulfilled'` — proving the
  whole chain actually runs, not just that each handler compiles. It uses a
  synthetic Clerk-style user id since it exercises the pipeline, not sign-in.

### Version pinning note

`graphile_worker` is pinned to `=0.13.1` (and its subcrates are pinned in
`Cargo.lock`): later patches moved to sqlx 0.9. Our own order queries share
this same `PgPool`, so they must stay on sqlx 0.8 too — currently they do.
Avoid a blanket `cargo update` (it would float the subcrates and could pull
in a newer major); update specific packages instead.

## HTTP API

Server functions are declared with explicit routes
(`#[post("/api/orders/start", ...)]`), so the wire API is stable instead of
macro-hashed. Authentication is handled entirely by Clerk in the browser
(clerk-js) — there are no `/api/auth/*` endpoints of our own:

| Method | Path | Body | Auth |
|---|---|---|---|
| POST | `/api/orders/start` | `{"order": {"item", "amount"}}` | Clerk session |
| GET | `/api/orders/list` | — | Clerk session |
| GET | `/api/orders/{id}` | — | Clerk session |

Protected endpoints require a verified Clerk session. In the browser that
rides on Clerk's session cookie automatically. To call them from a script,
mint a short-lived session token with Clerk's client (`use_auth().get_token()`
in the app, or Clerk's Backend API) and send it as `Authorization: Bearer
<token>`:

```bash
curl -H "Authorization: Bearer $CLERK_TOKEN" \
  -H 'content-type: application/json' \
  -d '{"order":{"item":"Widget","amount":10}}' \
  localhost:8080/api/orders/start
# "7db27402-..."

curl -H "Authorization: Bearer $CLERK_TOKEN" localhost:8080/api/orders/list
# [{"id":"7db27402-...","item":"Widget","amount":10,"status":"validating"}]
```

Unauthenticated calls to protected endpoints fail with an `unauthenticated`
error, which the UI handles by redirecting to Clerk's sign-in flow. Server
functions are the enforcement boundary — the client-side gating is only UX.

## Tests

```bash
docker compose up -d
cargo test --features server
```

One test: the end-to-end pipeline test (`src/jobs.rs`) that boots the
worker against real Postgres, enqueues an order for a synthetic user id, and
asserts it reaches `fulfilled` through every intermediate stage. It doesn't
touch Clerk — the pipeline is independent of how the user authenticated.

## Deploying

The `Dockerfile` here is a working multi-stage build: a `rust:1` stage
installs `dioxus-cli` and runs `dx bundle --platform web --release`, then a
slim `debian:bookworm-slim` runtime stage copies out just the server binary
and static assets.

Build it with this chapter directory as the context (it's a standalone
crate, so it doesn't need the rest of the workspace):

```bash
cd chapters/07-background-jobs
docker build --build-arg CLERK_PUBLISHABLE_KEY=pk_live_... -t myapp .
docker run -p 8080:8080 \
  --env DATABASE_URL=... \
  --env CLERK_SECRET_KEY=sk_live_... \
  myapp
```

Note `CLERK_PUBLISHABLE_KEY` is baked in at **build time** (`env!`), so it's a
`--build-arg` (the `Dockerfile` forwards it into the `dx bundle` step), not a
runtime `--env`; keep it consistent with the value the server uses. Only the
secret key is passed at runtime. In production, set `DATABASE_URL`
to a real Postgres instance, use your Clerk **live** keys (`pk_live_…` /
`sk_live_…`), add your deployed domain to the Clerk dashboard's allowed
origins, and put the app behind TLS — Clerk sets secure session cookies for
you once you're serving over https.

## You made it

You've built a full-stack Dioxus app from an empty `dx new` up through
hosted auth, per-user data, and a real background job pipeline — the same
shape of app you'd reach for at a startup or a side project. From here,
poking at this chapter directly (add a new job stage, add rate limiting to
`start_order`, swap the CSS for a UI library) is a good way to make it your
own.
