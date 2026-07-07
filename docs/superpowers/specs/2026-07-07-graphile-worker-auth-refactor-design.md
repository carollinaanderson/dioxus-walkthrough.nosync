# Design: graphile_worker_rs refactor + auth flow + named endpoints

Date: 2026-07-07
Status: Approved (Approach A)

## Goal

Refactor the duroxus template repo:

1. Replace duroxide/duroxide-pg durable orchestration with **graphile_worker_rs**
   (Postgres-backed job queue).
2. Add a **session-based auth flow** (users table, login/register/orders pages)
   to demonstrate multi-page routing in Dioxus.
3. Give server functions **explicit named HTTP endpoints** per the Dioxus docs.
4. Rewrite the **README** with a researched Deployment section (Hetzner/DO VPS,
   Railway, Fly.io) including database-setup guidance.

## Decisions made during brainstorming

- **Workflow model**: simplify to plain background jobs. The human
  approve/reject loop and the saga/refund path are dropped — orders run
  validate → charge → fulfill automatically and the UI shows live progress.
- **Auth depth**: users table in Postgres with argon2-hashed passwords,
  session cookie, protected orders page.
- **Pages**: `/login`, `/register`, `/` (orders, protected).
- **Deploy targets**: Hetzner/DO VPS, Railway, Fly.io. Brief note on why
  always-on Postgres matters for graphile_worker (LISTEN/NOTIFY; Neon
  scale-to-zero caveat) inside the database-setup guidance.
- **Approach A chosen**: embedded worker (single binary), chained jobs,
  DB-backed sessions.

## Architecture

**Process layout** — one binary, as today. `main.rs` boots `AppState`
(PgPool + graphile_worker `WorkerUtils`), spawns `worker.run()` as a
background tokio task, then serves the Dioxus router with `AppState` as an
`axum::Extension`. duroxide and duroxide-pg are removed entirely
(`src/workflow.rs` deleted, deps dropped from Cargo.toml).

**Jobs** — new `src/jobs.rs` with three `TaskHandler` structs sharing an
`OrderJob { order_id }` payload shape:

| Task | On run |
|---|---|
| `ValidateOrder` | set status `validating`, brief sleep, set `charging`, enqueue `ChargePayment` |
| `ChargePayment` | brief sleep, set `fulfilling`, enqueue `FulfillOrder` |
| `FulfillOrder` | brief sleep, set `fulfilled` |

Each job sleeps ~1–2s so the polling UI visibly walks the stages. The PgPool
reaches handlers via graphile_worker's extensions mechanism (exact mechanism
confirmed against crate docs during planning). If a job permanently fails
(after graphile_worker's default retries), status is set to `failed`.

## Data model

Migrations updated:

- `orders`: plain `id` (uuid) replaces `instance_id`; add `status` text
  (`queued | validating | charging | fulfilling | fulfilled | failed`); add
  `user_id` FK — orders belong to the logged-in user.
- `users`: `id uuid PK, username text unique, password_hash text, created_at`.
- graphile_worker creates/owns its own `graphile_worker` schema automatically.
- Sessions table: created by tower-sessions' sqlx Postgres store.

## Auth, routing, pages

**Routing** — Dioxus Router; `app.rs` becomes a shell with a `Route` enum:

- `/login` — `LoginPage`: username/password form → `login` server fn → navigate to `/`.
- `/register` — `RegisterPage`: same shape → `register` (hashes server-side, auto-login).
- `/` — `OrdersPage`: existing create-order + polling table UI, plus
  "logged in as X" header and logout button.

**Route guarding** — client-side: `OrdersPage` calls `current_user` on mount
and navigates to `/login` when `None`. The real boundary is server-side: every
order server fn extracts the session and errors if unauthenticated.

**Sessions** — `tower-sessions` + `tower-sessions-sqlx-store` (Postgres)
layered onto the axum router in `main.rs`. Login writes `user_id` into the
session; logout destroys it. Server fns access the session via the extractor
pattern in the `#[server]` attribute (same mechanism as `Extension<AppState>`).

**Auth server fns** — new `src/auth.rs`: `register`, `login`, `logout`,
`current_user`. Passwords hashed with `argon2` (server-only dep).
`list_orders`/`start_order`/`get_order_status` filter by session `user_id`.
Registration rejects duplicate usernames with a friendly error.

## Named HTTP endpoints

Each server fn gets an explicit path via the Dioxus `#[server]` attribute
(exact syntax verified against Dioxus 0.7 docs during planning):

| Server fn | Endpoint |
|---|---|
| `register` | `auth/register` |
| `login` | `auth/login` |
| `logout` | `auth/logout` |
| `current_user` | `auth/me` |
| `start_order` | `orders/start` |
| `list_orders` | `orders/list` |
| `get_order_status` | `orders/get` |

These land under Dioxus's API prefix (`/api/...`), giving stable curl-able
paths; the README shows a curl example.

## Error handling

- Server fns return `ServerFnResult`; auth failures return a distinct error
  the UI maps to a redirect-to-login.
- Job failures → `orders.status = failed` → red pill in the UI.

## Testing

- Replace the existing single Postgres integration test with one equivalent
  test in `jobs.rs`: boot `AppState` + worker against real Postgres (same
  docker-compose), register a user, start an order, poll until `fulfilled`,
  asserting row transitions along the way.
- Small unit test for the argon2 hash/verify round-trip in `auth.rs`.

## README rewrite

- Updated architecture description (graphile_worker replaces duroxide).
- Run/test instructions, endpoint table with curl example.
- New **Deployment** section (researched):
  - **Hetzner / DigitalOcean VPS** — docker-compose or systemd unit + managed
    or co-located Postgres.
  - **Railway** — Dockerfile deploy + Postgres addon.
  - **Fly.io** — fly.toml + Fly Postgres or Supabase.
  - Database-setup guidance including why graphile_worker wants an always-on
    Postgres (LISTEN/NOTIFY keeps a connection open) and the resulting Neon
    scale-to-zero caveat.
- Add a `Dockerfile` to the repo (needed by Railway/Fly instructions).

## Out of scope

- Human approval / saga compensation (removed by design).
- Separate worker binary (mentioned as a production note in the README only).
- OAuth/OIDC, password reset, CSRF hardening beyond the session cookie defaults.
