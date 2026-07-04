# Dioxus + duroxide PoC — "Order Approval" full-stack app

**Date:** 2026-07-04
**Status:** Approved (design)

## Goal

Scaffold a proof-of-concept full-stack [Dioxus](https://dioxuslabs.com) application whose
server functions act upon sample [duroxide](https://github.com/microsoft/duroxide) durable-execution
workflows, persisted through the [duroxide-pg](https://github.com/microsoft/duroxide-pg)
(`PostgresProvider`) backend. The demo showcases a human-in-the-loop **order approval** workflow
driven end-to-end from the browser.

## Research summary (up-to-date, July 2026)

- **Dioxus** — latest `0.7.x` (`v0.7.2`). Fullstack via the `#[server]` macro; server functions are
  called as ordinary `async` functions from components. Server-side shared state is provided with
  `provide_context` at serve config and read with `consume_context` inside `#[server]` functions.
  Cargo feature split: `web` (client/WASM) vs `server` (Axum + Tokio, non-WASM).
- **duroxide** — experimental, AI-authored durable-execution framework modeled on .NET's Durable Task
  Framework (`v0.1.x`). Core types: `OrchestrationContext` (`schedule_activity`, `schedule_timer`,
  `schedule_wait`, `select`/`join`), `ActivityRegistry`, `OrchestrationRegistry`, a `Runtime`/worker,
  and a `Client` control plane (`start_orchestration`, `wait_for_orchestration`, status queries,
  raise external event). Showcase patterns: activity chaining, fan-out/fan-in, durable timers,
  external events (human approval), built-in retry, saga-style compensation.
- **duroxide-pg** — `v0.1.34`. `PostgresProvider::new(conn_str)` and
  `PostgresProvider::new_with_schema(conn_str, Some("schema"))`. Automatic schema migration on
  startup. Standard `postgres://user:pass@host:port/db` connection string. Requires a live Postgres.

**Known risk:** duroxide's exact API surface varies between versions (e.g. `Runtime` vs `Worker`
constructor, precise `Client` method names, the raise-external-event signature). Implementation will
pin `main.rs` / `workflow.rs` against the actual installed crate source before claiming a build passes.

## Architecture & topology

- **Single Dioxus fullstack crate** with `web` + `server` features. `dx serve` runs everything —
  one process, one command.
- **Embedded runtime**: on server startup the process constructs the `PostgresProvider`, builds the
  activity + orchestration registries, starts the duroxide `Runtime`/worker, creates a duroxide
  `Client`, and injects the `Client` via `provide_context`. `#[server]` functions then reach the
  workflow control plane through `consume_context`.
- **Postgres 16 via docker-compose**; app reads `DATABASE_URL` from `.env`. Postgres owns both the
  durable workflow state (duroxide schema) and is the demo's source of truth.

## The workflow (duroxide) — `OrderApproval`

1. `validate_order` (activity)
2. `charge_payment` (activity, with retry / backoff)
3. `ctx.schedule_wait("approval")` — durably parks the orchestration until a human decides,
   raced (`select`) against a durable timer for auto-expiry.
4. On **approve** → `fulfill_order`.
5. On **reject** or **timer expiry** → **saga compensation** `refund_payment`.

Activities are intentionally simple stubs (log + short sleep + return) so the PoC stays focused on
orchestration mechanics rather than business logic.

## Server functions (Dioxus ↔ duroxide bridge)

All defined with `#[server]`, returning `ServerFnResult<T>`, reaching the `Client` via
`consume_context`:

- `start_order(order: OrderInput) -> String` — `client.start_orchestration(instance_id, "OrderApproval", input)`; returns the instance id.
- `get_order_status(instance_id: String) -> OrderStatusDto` — maps duroxide status/history to a serializable DTO (stage + terminal result).
- `submit_decision(instance_id: String, approve: bool)` — raises the `"approval"` external event with the decision payload.
- `list_orders() -> Vec<OrderStatusDto>` — lists instances for the dashboard.

Workflow/provider errors are mapped to friendly `ServerFnError` messages.

## UI (single page)

- **Create order** form → calls `start_order`, kicks off a workflow instance.
- **Orders table** polling `get_order_status` (via `use_action` on an interval) rendering each
  instance's stage: Validating → Charging → **Awaiting approval** → Fulfilled / Refunded.
- **Approve / Reject** buttons on rows in the *awaiting approval* state → call `submit_decision`;
  the row transitions live. This is the payoff: the browser drives a durable backend workflow.
- Per-row error state on failures.

## Project layout

```
duroxus/
├─ docker-compose.yml        # postgres:16 service
├─ .env / .env.example       # DATABASE_URL
├─ Cargo.toml                # dioxus 0.7 (web/server), duroxide, duroxide-pg, tokio, serde
├─ Dioxus.toml              # dioxus app config
├─ README.md                # setup + run steps
└─ src/
   ├─ main.rs               # launch; server startup: provider → registries → runtime → Client → provide_context
   ├─ app.rs                # UI components (form, table, decision buttons)
   ├─ server.rs             # #[server] functions
   └─ workflow.rs           # (server-only) orchestration + activities + registries + DTO mapping
```

## Error handling & testing

- Server functions return `ServerFnResult`; workflow/provider errors mapped to user-friendly
  messages; UI surfaces per-row error state.
- Testing scaled to a PoC:
  - A **workflow unit test** using duroxide's in-memory / SQLite provider (fast, no Postgres),
    covering both the **approve** path and the **reject / compensation** path.
  - **Manual end-to-end** run against docker Postgres, documented in the README.
  - `duroxide-pg` itself is exercised at runtime, not in CI.

## Out of scope (YAGNI)

- Auth / multi-user, real payment integration, production deployment config, separate scalable
  worker fleet (embedded runtime is sufficient for the PoC), and persistent order metadata beyond
  what duroxide's history already stores.
