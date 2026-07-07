# Duroxus — Dioxus + graphile_worker template

A full-stack [Dioxus](https://dioxuslabs.com) template where orders run through a
Postgres-backed [graphile_worker](https://github.com/leo91000/graphile_worker_rs)
job pipeline (validate → charge → fulfill) with live status in the UI,
session-based auth (register/login) guarding an orders page, and server
functions exposed on stable, curl-able HTTP endpoints.

Everything lives in **one Postgres**: app tables, the job queue
(`graphile_worker` schema), and the session store.

## Architecture

Single binary. On boot the server connects Postgres, runs sqlx migrations,
initializes graphile_worker (which migrates its own schema), spawns the worker
as a background tokio task, and serves the Dioxus app with tower-sessions +
shared-state layers.

| Path | Responsibility |
|---|---|
| `src/jobs.rs` | The three chained `TaskHandler`s (`validate_order` → `charge_payment` → `fulfill_order`) driving `orders.status`, plus the e2e pipeline test |
| `src/auth.rs` | Auth server fns (`register`/`login`/`logout`/`me`), argon2 hashing, `require_user_id` session guard |
| `src/users.rs`, `src/orders.rs` | Postgres stores (sqlx) |
| `src/server.rs` | Order server fns (start/list/get), which enqueue jobs via `WorkerUtils` |
| `src/state.rs` | `AppState` (pool + worker utils) threaded into server fns via `axum::Extension` |
| `src/app.rs`, `src/pages/` | Router: `/login`, `/register`, and the protected orders page at `/` |
| `migrations/` | sqlx migrations for `users` and `orders` |

Order statuses: `queued → validating → charging → fulfilling → fulfilled`
(or `failed`). Each job stamps its stage, sleeps ~1.2s to simulate work, and
enqueues the next job from inside the handler (`ctx.add_job`).

### Version pinning note

`graphile_worker` is pinned to `=0.13.1` (and its subcrates are pinned in
`Cargo.lock`): later patches moved to sqlx 0.9, while
`tower-sessions-sqlx-store` is still on sqlx 0.8 — with mismatched sqlx majors
they can't share one `PgPool`. When the sessions store catches up, unpin both.
Avoid blanket `cargo update` (it would float the subcrates back onto sqlx 0.9);
update specific packages instead.

## Run it

```bash
docker compose up -d          # Postgres 16 on localhost:5432
cp .env.example .env          # DATABASE_URL
dx serve                      # http://localhost:8080
```

Register an account, create an order, and watch its status pill walk the
pipeline (~4s). If you have a Postgres volume from the older duroxide version
of this template, reset it first: `docker compose down -v`.

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

Two tests: an argon2 hash round-trip and an end-to-end pipeline test that
boots the worker against real Postgres and asserts the order reaches
`fulfilled` through the intermediate stages.

## Deployment

### The build artifact

```bash
dx bundle --platform web --release
```

produces a self-contained server binary plus static assets under
`target/dx/duroxus/release/web/`. The provided `Dockerfile` does this in a
two-stage build (final image is `debian:bookworm-slim` + the bundle).
Configuration is entirely env-based: `DATABASE_URL`, `PORT` (default 8080),
`IP` (set to `0.0.0.0` in the image).

One constraint shapes every option below: **the worker is embedded in the web
server process**, so the process must be always-on. Platforms that
scale-to-zero or stop idle instances will silently pause job processing.

### Option 1: VPS (Hetzner Cloud / DigitalOcean)

The cheapest always-on home. A Hetzner CX22 (~€4/mo) or DO Basic Droplet
($6/mo) comfortably runs this app plus Postgres.

**docker-compose route** — extend the repo's compose file with an app service:

```yaml
services:
  app:
    build: .
    ports:
      - "8080:8080"
    environment:
      DATABASE_URL: postgres://duroxide:duroxide@postgres:5432/duroxide
    depends_on:
      postgres:
        condition: service_healthy
  postgres:
    # ... as in the repo's docker-compose.yml
```

**systemd route** — build the bundle (locally or in CI), copy the `web/`
directory to the box, and run the binary under a unit file:

```ini
[Service]
Environment=DATABASE_URL=postgres://...
Environment=PORT=8080
Environment=IP=127.0.0.1
WorkingDirectory=/opt/duroxus
ExecStart=/opt/duroxus/server
Restart=always
```

Put Caddy or nginx in front for TLS (and then set the session cookie
`with_secure(true)` in `main.rs`).

**Database**: a co-located Postgres container/package is fine at this scale.
DO Managed Postgres (from ~$15/mo) buys you backups and upgrades and is
always-on, so it's fully compatible with the worker — connect with
`?sslmode=require`.

### Option 2: Railway

- The `Dockerfile` is auto-detected. Add a **Postgres service** in the same
  project and set the app's `DATABASE_URL` to the reference variable
  `${{Postgres.DATABASE_URL}}`.
- Railway injects `PORT`; the server reads it, and the image sets
  `IP=0.0.0.0`.
- **Disable "App Sleeping"** (serverless mode) for the service — a slept app
  stops the embedded worker and jobs sit queued until the next HTTP request
  wakes it. Railway's Postgres is a plain always-on instance, so the queue
  side has no scale-to-zero surprises.

### Option 3: Fly.io

- `fly launch` picks up the Dockerfile; set `internal_port = 8080` in
  `fly.toml`.
- Fly's default is to stop idle machines. Keep the worker alive with:

```toml
[http_service]
  internal_port = 8080
  auto_stop_machines = "off"
  min_machines_running = 1
```

- **Database**: Fly Postgres (or Managed Postgres), which `fly postgres attach`
  wires up via `DATABASE_URL`, or an external managed provider (Supabase,
  Crunchy Bridge).

### Choosing the database (and the Neon caveat)

graphile_worker holds a `LISTEN` connection open and polls the queue (default
1s). Two consequences for "serverless" Postgres:

1. **Scale-to-zero never happens.** Neon-style autosuspend requires idle
   connections; the worker's LISTEN session plus polling keeps compute awake
   permanently, so you pay always-on prices with extra cold-start risk anyway.
2. **Transaction-mode poolers break it.** LISTEN/NOTIFY needs session state,
   which PgBouncer-style transaction pooling (including Neon's pooled
   connection string) doesn't preserve. If you must use Neon, use the
   **direct** (non-pooled) URL.

So: prefer a plain always-on Postgres — the VPS container, DO Managed,
Railway Postgres, or Fly Postgres all qualify. Keep
`worker concurrency × pool max_connections` under the provider's connection
limit (this template uses concurrency 2 and a 10-connection pool).

### Production notes

- Behind TLS, flip the session cookie to `with_secure(true)` in `main.rs`.
- The worker is embedded for simplicity. To scale job processing
  independently of the web tier, move `worker.run()` into a second binary
  that reuses `src/jobs.rs` and deploy it as a separate always-on process
  sharing the same `DATABASE_URL`.
