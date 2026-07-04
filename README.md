# Duroxus — Dioxus + duroxide Order-Approval PoC

A full-stack [Dioxus](https://dioxuslabs.com) app whose `#[server]` functions drive a
human-in-the-loop [duroxide](https://github.com/microsoft/duroxide) durable-execution
workflow, persisted in PostgreSQL via [duroxide-pg](https://github.com/microsoft/duroxide-pg).

The duroxide runtime is **embedded** in the Dioxus server process: on startup it builds a
`PostgresProvider`, registers the `OrderApproval` orchestration + its activities, starts the
runtime, and stashes a duroxide `Client` in a process global. Server functions use that client
to start orchestrations, read status, and raise the `approval` external event.

## Prerequisites

- Rust (stable) — built against 1.95, duroxide `0.1.29`, duroxide-pg `0.1.34`, Dioxus `0.7.9`.
- The Dioxus CLI: `cargo install dioxus-cli`
- Docker (for Postgres).

## Run

```bash
cp .env.example .env          # DATABASE_URL
docker compose up -d          # Postgres 16 on :5432 (schema auto-migrated on first run)
dx serve                      # builds client + server, embeds the duroxide runtime
```

Open the URL printed by `dx serve`.

## Demo

1. Enter an item + amount and click **Create order** → a durable `OrderApproval`
   orchestration starts: `ValidateOrder` → `ChargePayment` → *awaiting approval*.
2. The Orders table polls status every ~1.5s. A row parked at **Awaiting approval**
   shows **Approve** / **Reject** buttons.
3. **Approve** → the workflow runs `FulfillOrder`; the row becomes **Fulfilled**.
   **Reject** (or wait out the 120s auto-expiry timer) → saga compensation
   `RefundPayment` runs; the row becomes **Refunded**.

## Durability check

Create an order, leave it at *awaiting approval*, then `docker compose restart`. The
orchestration state survives in Postgres and resumes; approving it still completes the
workflow. (The dashboard's list of instance ids is kept in memory and resets when the app
restarts — the workflows themselves are the durable part, in Postgres.)

## Tests

```bash
cargo test --features test-support --no-default-features
```

Runs the `OrderApproval` orchestration through duroxide's in-memory SQLite provider
(no Postgres needed), covering both the approve → **FULFILLED** and reject → **REFUNDED**
paths. The Postgres provider is exercised at runtime via the demo above.

## Project layout

```
src/main.rs      # client launch + server entrypoint (env → workflow::init → serve)
src/app.rs       # UI: order form, polling status table, approve/reject
src/server.rs    # #[server] functions bridging the UI to the duroxide Client
src/workflow.rs  # (server-only) orchestration, activities, runtime bootstrap, globals
docs/API-NOTES.md            # confirmed duroxide/duroxide-pg API signatures used here
docs/superpowers/            # design spec + implementation plan
docker-compose.yml           # postgres:16
```

## Notes & PoC simplifications

- Because the activities are fast stubs, a `Running` instance is treated as
  "Awaiting approval" in the UI (it is effectively always parked at the approval wait).
- duroxide is an experimental, fast-moving framework; the exact API is pinned in
  `docs/API-NOTES.md` against the installed crate versions.
