# A hands-on Dioxus + Postgres tutorial

Welcome! This repo teaches you how to build a full-stack [Dioxus](https://dioxuslabs.com)
app — one small, working step at a time — by having you type the code
yourself, chapter by chapter.

You'll end up with a real app: an order-approval demo where users register,
log in, and submit orders that flow through a background job pipeline
(validate → charge → fulfill) with live status updates, all backed by one
Postgres database.

Nobody expects you to get everything right the first time. Each chapter
folder is a complete, working checkpoint — if you get stuck, you can always
compare your code against it, or just copy it and keep going.

## Before you start

You'll need:

- **Rust** — [rustup.rs](https://rustup.rs)
- **The Dioxus CLI** (`dx`) — `cargo install dioxus-cli`
- **Or** `cargo install cargo-binstall` and `cargo binstall dioxus-cli`
- **Docker** — for Postgres, starting in chapter 3 ([docker.com](https://www.docker.com/get-started/))
- **A Clerk account** — free at [clerk.com](https://clerk.com); starting in chapter 4 you'll need a publishable key and a secret key from your Clerk app

No prior Dioxus or async Rust experience assumed. We'll explain concepts as
we hit them.

## The chapters

| # | Chapter | What you'll learn |
|---|---|---|
| 1 | [hello-dioxus](chapters/01-hello-dioxus/README.md) | A minimal Dioxus web app — components, `rsx!`, running `dx serve` |
| 2 | [server-functions](chapters/02-server-functions/README.md) | Turning on the `server` feature and calling a `#[server]` function from the browser |
| 3 | [orders-database](chapters/03-orders-database/README.md) | Talking to Postgres with `sqlx`, and a real `orders` table |
| 4 | [user-accounts](chapters/04-user-accounts/README.md) | Wiring up Clerk — hosted email/password (and social) accounts with drop-in components; no local auth tables |
| 5 | [sessions](chapters/05-sessions/README.md) | Using Clerk sessions: gating pages with `SignedIn`/`SignedOut`, embedded sign-in/up, protected routes |
| 6 | [orders-per-user](chapters/06-orders-per-user/README.md) | Tying orders to the logged-in user |
| 7 | [background-jobs](chapters/07-background-jobs/README.md) | A `graphile_worker` job pipeline, live status polling, and shipping with Docker |

Each chapter is its own runnable project under `chapters/`, and the whole
thing is one Cargo workspace, so `cargo check --workspace` from the repo root
builds every chapter at once.

**Start here:** [chapters/01-hello-dioxus](chapters/01-hello-dioxus/README.md)
