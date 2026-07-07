# Duroxus tutorial workspace — design

## Goal

Refactor the single-crate Duroxus project into a Cargo workspace of 7
standalone, incrementally-built tutorial chapters. Each chapter is a complete
runnable Dioxus project; each chapter's README teaches the reader to hand-edit
their own copy of that chapter's code into the next chapter's functionality,
with the next chapter's folder serving as the answer key.

The final chapter (07) reproduces the current `main` branch functionality
(session auth + Postgres orders + graphile_worker pipeline) exactly.

## Workspace layout

```
duroxus/
├── Cargo.toml            # [workspace] members = ["chapters/*"]
├── README.md             # Welcome + prerequisites + chapter table, links to chapter 1
└── chapters/
    ├── 01-hello-dioxus/
    ├── 02-server-functions/
    ├── 03-orders-database/
    ├── 04-user-accounts/
    ├── 05-sessions/
    ├── 06-orders-per-user/
    └── 07-background-jobs/
```

Each `chapters/NN-name/` directory is self-contained:
- own `Cargo.toml` (package name `chNN-name`, e.g. `ch01-hello-dioxus`)
- own `Dioxus.toml`
- own `src/`
- from chapter 3 onward: own `migrations/`, `docker-compose.yml`, `.env.example`

A reader can `cd chapters/03-orders-database && dx serve` and it works with no
reference to any other chapter.

## Per-chapter Postgres isolation

Chapters 3–7 each need Postgres. To let a reader leave several chapters
running simultaneously (or jump backward) without volume/schema collisions,
each chapter's `docker-compose.yml` uses a distinct container name, db name,
volume name, and host port:

| Chapter | DB name | Port |
|---|---|---|
| 03-orders-database | `duroxus_ch03` | 5433 |
| 04-user-accounts | `duroxus_ch04` | 5434 |
| 05-sessions | `duroxus_ch05` | 5435 |
| 06-orders-per-user | `duroxus_ch06` | 5436 |
| 07-background-jobs | `duroxus_ch07` | 5437 |

Each chapter's `.env.example` points `DATABASE_URL` at its own port/db.

## Chapter contents

Each chapter is additive on top of the previous one's finished code.

1. **01-hello-dioxus** — minimal `dx new`-equivalent app: one page rendering
   static content, `web` feature only, no `server` feature, no backend.
2. **02-server-functions** — turns on the `server` feature; axum + tokio boot
   in `main.rs`; one `#[server]` fn callable from the page (no DB yet).
3. **03-orders-database** — adds Postgres + sqlx; `orders` table (no
   `user_id` yet); `orders.rs` store; start/list/get server fns; an orders
   page listing all orders (unscoped, no auth).
4. **04-user-accounts** — adds `users` table; `users.rs` store;
   register/login server fns with argon2 hashing. No sessions or route
   protection yet — register/login just return the created/matched user.
   Carries the argon2 hash round-trip unit test.
5. **05-sessions** — adds tower-sessions + its Postgres session store;
   `require_user_id` session guard; login/register pages; the orders page
   (still unscoped from ch03) becomes route-protected.
6. **06-orders-per-user** — adds `orders.user_id` FK; orders are scoped to
   the logged-in user in both the store queries and the server fns. This
   chapter's functionality matches current `main` minus graphile_worker.
7. **07-background-jobs** — adds graphile_worker; `jobs.rs` with the chained
   `validate_order → charge_payment → fulfill_order` handlers; status-polling
   UI; `Dockerfile` for deployment (moved here from repo root); the e2e
   pipeline test. This is the final state, equivalent to current `main`.

## README style

**Root `README.md`**: short welcome, prerequisites (Rust, `dx` CLI, Docker),
and a table of the 7 chapters with one-line descriptions, ending with "Start
at [chapters/01-hello-dioxus](chapters/01-hello-dioxus/README.md)."

**Each chapter `README.md`** follows this structure:
1. Friendly framing: what you'll learn in this chapter and why it matters
   (plain language, no unexplained jargon)
2. "Run it" — how to run *this chapter's* starting code (`dx serve`, and
   `docker compose up -d` where relevant)
3. Concept explanation for what's already in this chapter
4. A numbered, copy-pasteable walkthrough: edit your own working copy to
   build the next chapter's feature (migrations to add, structs/fns to write,
   snippets to paste, commands to run)
5. "Check your work" — compare your result against
   `chapters/NN+1-name/`, described as the answer key
6. "Next chapter →" link (omitted on chapter 07, which instead links back to
   the root README and mentions deployment via its `Dockerfile`)

## Migration of existing root files

- `Dockerfile` → moves to `chapters/07-background-jobs/Dockerfile`
- `docker-compose.yml`, `.env`, `.env.example` → replaced by per-chapter
  versions (chapters 3–7); root no longer has its own
- `migrations/` → split per chapter (03 gets `orders` only, 04/06 add to it,
  etc. — each chapter's `migrations/` contains the full set needed to run
  *that* chapter standalone)
- `src/*.rs` → distributed across chapters per the breakdown above
- Existing tests move with the code they test: argon2 round-trip → chapter
  04, e2e pipeline test → chapter 07
- Root `Cargo.lock` regenerated for the workspace; per-chapter `Cargo.lock`
  files are not used (workspace has one lockfile)
- `.dockerignore` moves to `chapters/07-background-jobs/.dockerignore`

## Out of scope

- No CI changes (none currently exist)
- No changes to the actual business logic/behavior of the final app —
  chapter 07 must behave identically to current `main`
- Not attempting to keep old root-level file paths working; this is a full
  restructure
