# dioxus-clerk Migration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the self-hosted `better-auth` crate with the hosted-auth crate `dioxus-clerk` across chapters 4–7, and update all affected READMEs.

**Architecture:** Auth moves out of the app's Postgres into Clerk's cloud. The browser mounts `<ClerkProvider>` (loads clerk-js) and uses Clerk's drop-in components/hooks; the server verifies the Clerk session cookie via the `ClerkAuthLayer` tower middleware and reads the verified user with `current_auth()`. No local auth tables remain.

**Tech Stack:** Rust, Dioxus 0.7 (fullstack + router), `dioxus-clerk` 0.1, axum 0.8, sqlx 0.8, Postgres, graphile_worker (ch7).

## Global Constraints

- `dioxus = { version = "0.7", ... }` — do not bump.
- `dioxus-clerk = "0.1"` — non-optional dependency (client components compile in every build); its `server` feature is enabled only under each chapter's `server` feature via `"dioxus-clerk/server"`.
- Env vars, exact names: `CLERK_PUBLISHABLE_KEY` (build-time, read with `env!` in `app.rs`), `CLERK_SECRET_KEY` (runtime, server-only, read by `ClerkAuthLayer::from_env`). Remove `BETTER_AUTH_SECRET` everywhere. Keep `DATABASE_URL`.
- Clerk user id is a `String` (e.g. `user_2abc…`). `orders.user_id` is plain `TEXT NOT NULL` — no FK to any local `users` table.
- Ch7 keeps the `graphile_worker` / `graphile_worker_ctx` / `graphile_worker_database` version pins and the sqlx 0.8 pin; only the *rationale comment* changes (no longer about better-auth sharing the pool).
- Server-side auth API: `dioxus_clerk::server::current_auth()` returns `Result<_, ClerkError>` with a `.user_id: String` field; `ClerkError` converts into `ServerFnError`. It is **synchronous** in the crate's demo (`let auth = current_auth()?;`). If the compiler reports it is a future, add `.await`.
- Verification is **best-effort** (per the spec). No live Clerk account is needed to compile; `env!("CLERK_PUBLISHABLE_KEY")` only needs the var present at build time — export a dummy `pk_test_dummy` for checks. The known risk is `dioxus-clerk`'s dependency tree on the wasm client build; if the `web` target fails to compile due to a transitive crate (e.g. tokio), record it in the chapter README's notes and the final summary rather than reworking the crate.

Check commands used throughout (run from repo root):

```bash
# server build of one chapter
CLERK_PUBLISHABLE_KEY=pk_test_dummy cargo check -p <pkg> --no-default-features --features server
# web/client build of one chapter (default features)
CLERK_PUBLISHABLE_KEY=pk_test_dummy cargo check -p <pkg>
```

Package names: `ch04-user-accounts`, `ch05-sessions`, `ch06-orders-per-user`, `ch07-background-jobs`.

---

### Task 1: Chapter 4 — user-accounts

The simplest chapter: no protected server fn, so no `require_user_id`. `auth.rs` is deleted entirely; the App is rebuilt around Clerk components. Global orders (unchanged, no auth) show only when signed in.

**Files:**
- Modify: `chapters/04-user-accounts/Cargo.toml`
- Modify: `chapters/04-user-accounts/.env.example`
- Delete: `chapters/04-user-accounts/migrations/0002_better_auth.sql`
- Delete: `chapters/04-user-accounts/src/auth.rs`
- Modify: `chapters/04-user-accounts/src/main.rs`
- Modify: `chapters/04-user-accounts/src/state.rs`
- Rewrite: `chapters/04-user-accounts/src/app.rs`
- Modify: `chapters/04-user-accounts/README.md`

**Interfaces:**
- Produces: no cross-chapter interface. `AppState { pool: PgPool }` (server-only). App uses `dioxus_clerk::{ClerkProvider, SignedIn, SignedOut, SignInButton, SignUpButton, UserButton}`.

- [ ] **Step 1: Cargo.toml — swap the dependency**

In `[dependencies]`, delete the `better-auth = …` line and add after the `serde_json` line:

```toml
dioxus-clerk = "0.1"
```

Change the `server` feature line to drop `"dep:better-auth"` and add `"dioxus-clerk/server"`:

```toml
server = ["dioxus/server", "dep:axum", "dep:tokio", "dep:sqlx", "dep:uuid", "dep:dotenvy", "dioxus-clerk/server"]
```

- [ ] **Step 2: .env.example — swap secrets**

Replace the whole file with:

```
DATABASE_URL=postgres://myapp:myapp@localhost:5434/myapp_ch04
# Clerk keys — create a free app at https://dashboard.clerk.com and copy these
# from "API keys". The publishable key is baked into the client at build time
# (env! in app.rs); the secret key is read at runtime on the server only and
# must never reach the browser bundle.
CLERK_PUBLISHABLE_KEY=pk_test_replace_me
CLERK_SECRET_KEY=sk_test_replace_me
```

- [ ] **Step 3: Delete better-auth migration and auth module**

```bash
git rm chapters/04-user-accounts/migrations/0002_better_auth.sql
git rm chapters/04-user-accounts/src/auth.rs
```

- [ ] **Step 4: main.rs — drop `mod auth`, add the Clerk layer**

Replace the file with:

```rust
mod app;
mod server;

#[cfg(feature = "server")]
mod orders;

#[cfg(feature = "server")]
mod state;

use app::App;

fn main() {
    // Client entrypoint.
    #[cfg(not(feature = "server"))]
    dioxus::launch(App);

    // Server entrypoint: connect Postgres, run migrations, and serve the app.
    // `ClerkAuthLayer` verifies the Clerk session cookie on every request so
    // server functions can read the caller's identity via `current_auth()`.
    #[cfg(feature = "server")]
    dioxus::serve(|| async {
        let state = state::AppState::new().await;
        let clerk = dioxus_clerk::server::ClerkAuthLayer::from_env()
            .expect("CLERK_SECRET_KEY must be set (see .env)");
        Ok(dioxus::server::router(App)
            .layer(clerk)
            .layer(axum::Extension(state)))
    });
}
```

- [ ] **Step 5: state.rs — remove better-auth**

Replace the file with:

```rust
//! Server-only shared `#[server]` state, threaded via `axum::Extension`:
//! just the Postgres pool now. Accounts and sessions live in Clerk, not here.

use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
}

impl AppState {
    /// Connect Postgres and run our own migrations.
    pub async fn new() -> Self {
        dotenvy::dotenv().ok();
        let database_url =
            std::env::var("DATABASE_URL").expect("DATABASE_URL must be set (see .env)");

        let pool = PgPoolOptions::new()
            .max_connections(10)
            .connect(&database_url)
            .await
            .expect("failed to connect to postgres");
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .expect("failed to run migrations");

        Self { pool }
    }
}
```

- [ ] **Step 6: app.rs — rebuild around Clerk components**

Replace everything above `pub const CSS` (lines 1–170) with:

```rust
#![allow(non_snake_case)]
//! One page. Clerk owns accounts now: signed-out visitors get sign-in /
//! sign-up buttons; signed-in visitors get a Clerk `UserButton` (avatar menu
//! with sign-out) and the orders section from chapter 3. The orders list is
//! still global — chapter 6 scopes it per user.

use dioxus::prelude::*;
use dioxus_clerk::{ClerkProvider, SignInButton, SignUpButton, SignedIn, SignedOut, UserButton};

use crate::server::{list_orders, start_order, OrderDto, OrderInput};

pub fn App() -> Element {
    rsx! {
        style { {CSS} }
        ClerkProvider { publishable_key: env!("CLERK_PUBLISHABLE_KEY"),
            main { class: "wrap",
                header { class: "nav",
                    div {
                        h1 { "MyApp" }
                        p { class: "sub", "Chapter 4: user accounts via Clerk." }
                    }
                    div { class: "row",
                        SignedOut {
                            SignInButton { class: "primary", "Sign in" }
                            SignUpButton { class: "ghost", "Create account" }
                        }
                        SignedIn { UserButton {} }
                    }
                }

                SignedOut {
                    section { class: "card",
                        p { class: "muted", "Sign in or create an account to place orders." }
                    }
                }

                SignedIn { OrdersSection {} }
            }
        }
    }
}

#[component]
fn OrdersSection() -> Element {
    let mut item = use_signal(|| "Widget".to_string());
    let mut amount = use_signal(|| "10".to_string());
    let mut orders = use_signal(Vec::<OrderDto>::new);
    let mut order_error = use_signal(|| Option::<String>::None);

    let refresh_orders = move |_| async move {
        match list_orders().await {
            Ok(list) => {
                orders.set(list);
                order_error.set(None);
            }
            Err(e) => order_error.set(Some(e.to_string())),
        }
    };

    let create_order = move |_| async move {
        let amt = amount().trim().parse::<u32>().unwrap_or(0);
        match start_order(OrderInput { item: item(), amount: amt }).await {
            Ok(_) => {
                order_error.set(None);
                if let Ok(list) = list_orders().await {
                    orders.set(list);
                }
            }
            Err(e) => order_error.set(Some(e.to_string())),
        }
    };

    use_future(move || async move {
        if let Ok(list) = list_orders().await {
            orders.set(list);
        }
    });

    rsx! {
        section { class: "card",
            h2 { "New order" }
            div { class: "row",
                input {
                    value: "{item}",
                    oninput: move |e| item.set(e.value()),
                    placeholder: "Item",
                }
                input {
                    value: "{amount}",
                    oninput: move |e| amount.set(e.value()),
                    placeholder: "Amount",
                }
                button { class: "primary", onclick: create_order, "Create order" }
                button { onclick: refresh_orders, "Refresh" }
            }
            if let Some(e) = order_error() {
                p { class: "err", "Error: {e}" }
            }
        }

        section { class: "card",
            h2 { "Orders" }
            if orders().is_empty() {
                p { class: "muted", "No orders yet — create one above." }
            } else {
                table {
                    thead {
                        tr { th { "Item" } th { "Amount" } th { "Id" } th { "Status" } }
                    }
                    tbody {
                        for o in orders() {
                            tr {
                                td { "{o.item}" }
                                td { "{o.amount}" }
                                td { class: "mono", "{o.id}" }
                                td { "{o.status}" }
                            }
                        }
                    }
                }
            }
        }
    }
}
```

Keep the existing `pub const CSS: &str = r#"…"#;` block unchanged.

- [ ] **Step 7: README.md — rewrite the auth story**

Update `chapters/04-user-accounts/README.md`: replace all better-auth.rs prose with the Clerk flow — creating a Clerk app, copying the publishable + secret keys into `.env`, `ClerkProvider` wrapping the app, `SignedIn`/`SignedOut` gating, `SignInButton`/`SignUpButton`/`UserButton`. State that accounts and sessions now live in Clerk (no local tables), which is why `0002_better_auth.sql` is gone. Keep the chapter's structure, headings, and run instructions; only the auth content changes.

- [ ] **Step 8: Check both builds**

```bash
CLERK_PUBLISHABLE_KEY=pk_test_dummy cargo check -p ch04-user-accounts --no-default-features --features server
CLERK_PUBLISHABLE_KEY=pk_test_dummy cargo check -p ch04-user-accounts
```

Expected: both compile. If the web build fails on a transitive `dioxus-clerk` dependency, note it (see Global Constraints) and continue.

- [ ] **Step 9: Commit**

```bash
git add -A chapters/04-user-accounts
git commit -m "feat(ch04): migrate user-accounts from better-auth.rs to dioxus-clerk"
```

---

### Task 2: Chapter 5 — sessions

Router chapter. `pages/login.rs` and `pages/register.rs` become embedded Clerk widget routes; the orders route is guarded with Clerk gating; `auth.rs` shrinks to the server-only `require_user_id()`.

**Files:**
- Modify: `chapters/05-sessions/Cargo.toml`
- Modify: `chapters/05-sessions/.env.example`
- Delete: `chapters/05-sessions/migrations/0002_better_auth.sql`
- Rewrite: `chapters/05-sessions/src/auth.rs`
- Rewrite: `chapters/05-sessions/src/state.rs`
- Modify: `chapters/05-sessions/src/main.rs`
- Modify: `chapters/05-sessions/src/app.rs`
- Rewrite: `chapters/05-sessions/src/pages/login.rs`
- Rewrite: `chapters/05-sessions/src/pages/register.rs`
- Rewrite: `chapters/05-sessions/src/pages/orders.rs`
- Modify: `chapters/05-sessions/src/server.rs`
- Modify: `chapters/05-sessions/README.md`

**Interfaces:**
- Produces: `crate::auth::require_user_id() -> Result<String, ServerFnError>` (server-only, **no args, not async**). Callers use `crate::auth::require_user_id()?`.

- [ ] **Step 1: Cargo.toml — swap the dependency**

Same edit as Task 1 Step 1 (delete `better-auth`, add `dioxus-clerk = "0.1"`, set the `server` feature to include `"dioxus-clerk/server"` and drop `"dep:better-auth"`). Match this chapter's existing feature line, changing only the auth entries.

- [ ] **Step 2: .env.example — swap secrets**

Replace with the same block as Task 1 Step 2, but keep this chapter's `DATABASE_URL` value (`…/myapp_ch05`).

- [ ] **Step 3: Delete better-auth migration**

```bash
git rm chapters/05-sessions/migrations/0002_better_auth.sql
```

- [ ] **Step 4: auth.rs — shrink to the server-side boundary**

Replace the whole file with:

```rust
//! Server-side auth boundary. Clerk verifies the session cookie in the
//! `ClerkAuthLayer` middleware (wired in `main.rs`); here we read the verified
//! identity out of the current request. This is the real enforcement point for
//! protected server fns — the client-side gating is only UX.

/// The Clerk user id for the current request, or an error if unauthenticated.
#[cfg(feature = "server")]
pub fn require_user_id() -> Result<String, dioxus::prelude::ServerFnError> {
    Ok(dioxus_clerk::server::current_auth()?.user_id)
}
```

- [ ] **Step 5: state.rs — remove better-auth**

Replace with the same `AppState { pool }` implementation as Task 1 Step 5.

- [ ] **Step 6: main.rs — add the Clerk layer**

Edit the `#[cfg(feature = "server")]` serve block to add the layer (leave the client entrypoint and `mod` lines as they are):

```rust
    #[cfg(feature = "server")]
    dioxus::serve(|| async {
        let state = state::AppState::new().await;
        let clerk = dioxus_clerk::server::ClerkAuthLayer::from_env()
            .expect("CLERK_SECRET_KEY must be set (see .env)");
        Ok(dioxus::server::router(App)
            .layer(clerk)
            .layer(axum::Extension(state)))
    });
```

- [ ] **Step 7: app.rs — wrap the router in ClerkProvider**

Replace the `App` function (keep the `Route` enum and CSS):

```rust
pub fn App() -> Element {
    rsx! {
        style { {CSS} }
        dioxus_clerk::ClerkProvider { publishable_key: env!("CLERK_PUBLISHABLE_KEY"),
            Router::<Route> {}
        }
    }
}
```

- [ ] **Step 8: pages/login.rs — embedded Clerk SignIn widget**

Replace the whole file with:

```rust
#![allow(non_snake_case)]
//! Sign-in route: Clerk's embedded `<SignIn />` widget. Path routing keeps
//! Clerk's own child paths (e.g. SSO callbacks) under `/login`.

use dioxus::prelude::*;
use dioxus_clerk::SignIn;

#[component]
pub fn LoginPage() -> Element {
    rsx! {
        main { class: "wrap narrow",
            h1 { "Sign in" }
            p { class: "sub", "MyApp order pipeline demo" }
            SignIn {}
        }
    }
}
```

- [ ] **Step 9: pages/register.rs — embedded Clerk SignUp widget**

Replace the whole file with:

```rust
#![allow(non_snake_case)]
//! Sign-up route: Clerk's embedded `<SignUp />` widget.

use dioxus::prelude::*;
use dioxus_clerk::SignUp;

#[component]
pub fn RegisterPage() -> Element {
    rsx! {
        main { class: "wrap narrow",
            h1 { "Create account" }
            p { class: "sub", "MyApp order pipeline demo" }
            SignUp {}
        }
    }
}
```

- [ ] **Step 10: pages/orders.rs — gate with Clerk instead of a manual guard**

Replace the whole file with:

```rust
#![allow(non_snake_case)]
//! Protected orders page. `SignedOut` + `RedirectToSignIn` send anonymous
//! visitors to the sign-in route; the real orders UI only renders inside
//! `SignedIn`. Server fns still enforce auth themselves via `require_user_id`.
//! Every logged-in user still sees the same global order list — chapter 6
//! scopes this per user.

use dioxus::prelude::*;
use dioxus_clerk::{RedirectToSignIn, SignedIn, SignedOut, UserButton};

use crate::server::{list_orders, start_order, OrderDto, OrderInput};

#[component]
pub fn OrdersPage() -> Element {
    rsx! {
        SignedOut { RedirectToSignIn {} }
        SignedIn { OrdersView {} }
    }
}

#[component]
fn OrdersView() -> Element {
    let mut item = use_signal(|| "Widget".to_string());
    let mut amount = use_signal(|| "10".to_string());
    let mut orders = use_signal(Vec::<OrderDto>::new);
    let mut error = use_signal(|| Option::<String>::None);

    let refresh = move |_| async move {
        match list_orders().await {
            Ok(list) => {
                orders.set(list);
                error.set(None);
            }
            Err(e) => error.set(Some(e.to_string())),
        }
    };

    use_future(move || async move {
        if let Ok(list) = list_orders().await {
            orders.set(list);
        }
    });

    let create = move |_| async move {
        let amt = amount().trim().parse::<u32>().unwrap_or(0);
        match start_order(OrderInput { item: item(), amount: amt }).await {
            Ok(_) => {
                error.set(None);
                if let Ok(list) = list_orders().await {
                    orders.set(list);
                }
            }
            Err(e) => error.set(Some(e.to_string())),
        }
    };

    rsx! {
        main { class: "wrap",
            header { class: "nav",
                div {
                    h1 { "MyApp" }
                    p { class: "sub", "Chapter 5: Clerk sessions protect this page." }
                }
                div { class: "row",
                    UserButton {}
                }
            }

            section { class: "card",
                h2 { "New order" }
                div { class: "row",
                    input {
                        value: "{item}",
                        oninput: move |e| item.set(e.value()),
                        placeholder: "Item",
                    }
                    input {
                        r#type: "number",
                        value: "{amount}",
                        oninput: move |e| amount.set(e.value()),
                        placeholder: "Amount",
                    }
                    button { class: "primary", onclick: create, "Create order" }
                    button { onclick: refresh, "Refresh" }
                }
            }

            if let Some(e) = error() {
                p { class: "err", "Error: {e}" }
            }

            section { class: "card",
                h2 { "Orders" }
                if orders().is_empty() {
                    p { class: "muted", "No orders yet — create one above." }
                } else {
                    table {
                        thead {
                            tr { th { "Item" } th { "Amount" } th { "Id" } th { "Status" } }
                        }
                        tbody {
                            for o in orders() {
                                tr { key: "{o.id}",
                                    td { "{o.item}" }
                                    td { "{o.amount}" }
                                    td { class: "mono", "{o.id}" }
                                    td { "{o.status}" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
```

- [ ] **Step 11: server.rs — update the `require_user_id` call sites**

In `start_order`, change `crate::auth::require_user_id(&state).await?;` to:

```rust
    crate::auth::require_user_id()?;
```

In `list_orders`, change `crate::auth::require_user_id(&state).await?;` to:

```rust
    crate::auth::require_user_id()?;
```

(Leave the `state: axum::Extension<...>` params — they are still used for `state.pool`.)

- [ ] **Step 12: README.md — rewrite the auth story**

Rewrite `chapters/05-sessions/README.md` around Clerk: `ClerkProvider` at the router root, embedded `<SignIn />` / `<SignUp />` routes, `SignedIn`/`SignedOut` + `RedirectToSignIn` route gating, `UserButton` for sign-out, and the server-side `require_user_id()` boundary reading `current_auth()`. Explain the two-layer model (client gating = UX, server fn = enforcement). Keep run instructions; only auth content changes.

- [ ] **Step 13: Check both builds**

```bash
CLERK_PUBLISHABLE_KEY=pk_test_dummy cargo check -p ch05-sessions --no-default-features --features server
CLERK_PUBLISHABLE_KEY=pk_test_dummy cargo check -p ch05-sessions
```

Expected: both compile (see best-effort note for the web build).

- [ ] **Step 14: Commit**

```bash
git add -A chapters/05-sessions
git commit -m "feat(ch05): migrate sessions from better-auth.rs to dioxus-clerk"
```

---

### Task 3: Chapter 6 — orders-per-user

Adds real per-user scoping. Same auth swap as ch5, plus the `user_id` migration loses its FK to the (now-gone) `users` table, and `require_user_id()`'s return is used to scope queries.

**Files:**
- Modify: `chapters/06-orders-per-user/Cargo.toml`
- Modify: `chapters/06-orders-per-user/.env.example`
- Delete: `chapters/06-orders-per-user/migrations/0002_better_auth.sql`
- Rename+rewrite: `migrations/0003_add_user_id_to_orders.sql` → `migrations/0002_add_user_id_to_orders.sql`
- Rewrite: `chapters/06-orders-per-user/src/auth.rs`
- Rewrite: `chapters/06-orders-per-user/src/state.rs`
- Modify: `chapters/06-orders-per-user/src/main.rs`
- Modify: `chapters/06-orders-per-user/src/app.rs`
- Rewrite: `chapters/06-orders-per-user/src/pages/login.rs`
- Rewrite: `chapters/06-orders-per-user/src/pages/register.rs`
- Rewrite: `chapters/06-orders-per-user/src/pages/orders.rs`
- Modify: `chapters/06-orders-per-user/src/server.rs`
- Modify: `chapters/06-orders-per-user/README.md`

**Interfaces:**
- Consumes: `crate::auth::require_user_id()` (as defined in Task 2).
- Produces: orders scoped by the Clerk user id string.

- [ ] **Step 1: Cargo.toml — swap the dependency** (same edit as Task 2 Step 1, this chapter's feature line).

- [ ] **Step 2: .env.example — swap secrets** (same block; keep `…/myapp_ch06`).

- [ ] **Step 3: Delete better-auth migration, renumber the user_id migration**

```bash
git rm chapters/06-orders-per-user/migrations/0002_better_auth.sql
git mv chapters/06-orders-per-user/migrations/0003_add_user_id_to_orders.sql \
       chapters/06-orders-per-user/migrations/0002_add_user_id_to_orders.sql
```

Then replace the renamed file's contents with:

```sql
-- Orders belong to a user. With Clerk, users live in Clerk's cloud, not in our
-- database, so user_id is just the Clerk user id string (e.g. "user_2abc…") —
-- there is no local users table to reference.
ALTER TABLE orders ADD COLUMN user_id TEXT;

-- This is a fresh tutorial database, so `orders` is empty here — no rows to
-- backfill. In a real app with existing data you'd backfill user_id on
-- existing rows before adding the NOT NULL constraint.
ALTER TABLE orders ALTER COLUMN user_id SET NOT NULL;

CREATE INDEX IF NOT EXISTS orders_user_created_idx ON orders (user_id, created_at DESC);
```

- [ ] **Step 4: auth.rs — same server-side boundary** as Task 2 Step 4.

- [ ] **Step 5: state.rs — remove better-auth** (same `AppState { pool }` as Task 1 Step 5).

- [ ] **Step 6: main.rs — add the Clerk layer** (same serve-block edit as Task 2 Step 6).

- [ ] **Step 7: app.rs — wrap router in ClerkProvider** (same `App` edit as Task 2 Step 7; keep the `Route` enum, `status_class` if present, and CSS).

- [ ] **Step 8: pages/login.rs** — same embedded `<SignIn />` widget as Task 2 Step 8.

- [ ] **Step 9: pages/register.rs** — same embedded `<SignUp />` widget as Task 2 Step 9.

- [ ] **Step 10: pages/orders.rs — gate with Clerk**

Use the same structure as Task 2 Step 10, with this chapter's copy. Change the subtitle to `"Chapter 6: orders belong to whoever created them."` and the orders card heading to `"Your orders"`. Keep the `SignedOut { RedirectToSignIn {} }` / `SignedIn { OrdersView {} }` split and `UserButton` in the header.

- [ ] **Step 11: server.rs — scope by the Clerk user id**

In `start_order`, replace `crate::auth::require_user_id(&state).await?;` and the insert with:

```rust
    let user_id = crate::auth::require_user_id()?;
    let row = crate::orders::insert(&state.pool, &user_id, &order.item, order.amount)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;
```

In `list_orders`, replace the guard + list with:

```rust
    let user_id = crate::auth::require_user_id()?;
    let rows = crate::orders::list_for_user(&state.pool, &user_id)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;
```

(If this chapter's `orders.rs` store fns differ from ch7's `insert(pool, user_id, …)` / `list_for_user(pool, user_id)` signatures, check `chapters/06-orders-per-user/src/orders.rs` and match whatever per-user signatures it already defines — this chapter is where per-user scoping is introduced, so those fns should already take `user_id`.)

- [ ] **Step 12: README.md — rewrite the auth + per-user story**

Rewrite `chapters/06-orders-per-user/README.md`: Clerk gating (as ch5), plus that `require_user_id()` now returns the Clerk user id used to scope orders, and that `user_id` is a plain `TEXT` column holding the Clerk id (no FK to a local users table, because there isn't one). Note the migration renumber (0002).

- [ ] **Step 13: Check both builds**

```bash
CLERK_PUBLISHABLE_KEY=pk_test_dummy cargo check -p ch06-orders-per-user --no-default-features --features server
CLERK_PUBLISHABLE_KEY=pk_test_dummy cargo check -p ch06-orders-per-user
```

- [ ] **Step 14: Commit**

```bash
git add -A chapters/06-orders-per-user
git commit -m "feat(ch06): migrate orders-per-user from better-auth.rs to dioxus-clerk"
```

---

### Task 4: Chapter 7 — background-jobs

Same auth swap as ch6, preserving all graphile_worker wiring. `AppState` keeps `pool` + `worker`; `main.rs` keeps the worker handle tuple.

**Files:**
- Modify: `chapters/07-background-jobs/Cargo.toml`
- Modify: `chapters/07-background-jobs/.env.example`
- Delete: `chapters/07-background-jobs/migrations/0002_better_auth.sql`
- Rename+rewrite: `migrations/0003_add_user_id_to_orders.sql` → `migrations/0002_add_user_id_to_orders.sql`
- Rewrite: `chapters/07-background-jobs/src/auth.rs`
- Modify: `chapters/07-background-jobs/src/state.rs`
- Modify: `chapters/07-background-jobs/src/main.rs`
- Modify: `chapters/07-background-jobs/src/app.rs`
- Rewrite: `chapters/07-background-jobs/src/pages/login.rs`
- Rewrite: `chapters/07-background-jobs/src/pages/register.rs`
- Rewrite: `chapters/07-background-jobs/src/pages/orders.rs`
- Modify: `chapters/07-background-jobs/src/server.rs`
- Modify: `chapters/07-background-jobs/README.md`

**Interfaces:**
- Consumes: `crate::auth::require_user_id()` (Task 2).
- Produces: `AppState { pool, worker }` (server-only).

- [ ] **Step 1: Cargo.toml — swap the dependency, keep pins, fix the comment**

Delete the `better-auth = …` line. Add `dioxus-clerk = "0.1"` to `[dependencies]`. In the `server` feature list, drop `"dep:better-auth"` and add `"dioxus-clerk/server"`. Rewrite the sqlx-pin comment (currently references better-auth.rs) to:

```toml
# =0.13.1: last version on sqlx 0.8; later graphile_worker releases moved to
# sqlx 0.9. Keep this pin — graphile_worker shares one PgPool with our own
# order queries, so both must agree on sqlx's major version.
```

- [ ] **Step 2: .env.example — swap secrets** (same block; keep `…/myapp_ch07`).

- [ ] **Step 3: Delete better-auth migration, renumber the user_id migration**

```bash
git rm chapters/07-background-jobs/migrations/0002_better_auth.sql
git mv chapters/07-background-jobs/migrations/0003_add_user_id_to_orders.sql \
       chapters/07-background-jobs/migrations/0002_add_user_id_to_orders.sql
```

Replace the renamed file's contents with the same SQL as Task 3 Step 3.

- [ ] **Step 4: auth.rs — same server-side boundary** as Task 2 Step 4.

- [ ] **Step 5: state.rs — remove better-auth, keep the worker**

Replace with:

```rust
//! Server-only shared state, threaded into `#[server]` functions via
//! `axum::Extension` instead of process-global statics.

use graphile_worker::runner::WorkerRuntimeError;
use graphile_worker::{WorkerOptions, WorkerUtils};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use tokio::task::JoinHandle;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub worker: WorkerUtils,
}

impl AppState {
    /// Connect Postgres, run app migrations, and initialize the
    /// graphile_worker worker (which creates/migrates its own
    /// `graphile_worker` schema). Returns the state plus the worker
    /// background task. Accounts and sessions live in Clerk, not here.
    pub async fn new() -> (Self, JoinHandle<Result<(), WorkerRuntimeError>>) {
        dotenvy::dotenv().ok();
        let database_url =
            std::env::var("DATABASE_URL").expect("DATABASE_URL must be set (see .env)");

        let pool = PgPoolOptions::new()
            .max_connections(10)
            .connect(&database_url)
            .await
            .expect("failed to connect to postgres");
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .expect("failed to run migrations");

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
        (
            Self { pool, worker: worker_utils },
            worker_handle,
        )
    }
}
```

- [ ] **Step 6: main.rs — add the Clerk layer, keep the worker tuple**

Edit the server serve block:

```rust
    #[cfg(feature = "server")]
    dioxus::serve(|| async {
        let (state, _) = state::AppState::new().await;
        let clerk = dioxus_clerk::server::ClerkAuthLayer::from_env()
            .expect("CLERK_SECRET_KEY must be set (see .env)");
        Ok(dioxus::server::router(App)
            .layer(clerk)
            .layer(axum::Extension(state)))
    });
```

- [ ] **Step 7: app.rs — wrap router in ClerkProvider** (same `App` edit as Task 2 Step 7; keep the `Route` enum, `status_class`, and CSS).

- [ ] **Step 8: pages/login.rs** — same embedded `<SignIn />` widget as Task 2 Step 8.

- [ ] **Step 9: pages/register.rs** — same embedded `<SignUp />` widget as Task 2 Step 9.

- [ ] **Step 10: pages/orders.rs — gate with Clerk, keep live polling**

Rebuild as `SignedOut { RedirectToSignIn {} }` + `SignedIn { OrdersView {} }`. Inside `OrdersView`, keep this chapter's live status-polling loop (the `sleep_ms` helper and the ~1.5s `use_future` poll loop) and `status_class` styling. Remove the manual `current_user()`/`user` signal and the `UNAUTHENTICATED` redirect branch — gating is now Clerk's job; on an auth error the poll loop just surfaces the error. Header uses `UserButton {}`. Full shape:

```rust
#![allow(non_snake_case)]
//! Protected orders page: create an order, watch the graphile_worker pipeline
//! advance its status via polling. Clerk gates the page (`RedirectToSignIn`);
//! server fns enforce auth themselves via `require_user_id`.

use dioxus::prelude::*;
use dioxus_clerk::{RedirectToSignIn, SignedIn, SignedOut, UserButton};

use crate::app::status_class;
use crate::server::{list_orders, start_order, OrderDto, OrderInput};

/// Interval sleep for the polling loop. The loop only ever runs on the wasm
/// client (`use_future` does not run during native SSR); the non-wasm arm just
/// has to compile, so it parks forever without pulling in a native timer crate.
async fn sleep_ms(_ms: u32) {
    #[cfg(target_arch = "wasm32")]
    gloo_timers::future::TimeoutFuture::new(_ms).await;
    #[cfg(not(target_arch = "wasm32"))]
    std::future::pending::<()>().await;
}

#[component]
pub fn OrdersPage() -> Element {
    rsx! {
        SignedOut { RedirectToSignIn {} }
        SignedIn { OrdersView {} }
    }
}

#[component]
fn OrdersView() -> Element {
    let mut item = use_signal(|| "Widget".to_string());
    let mut amount = use_signal(|| "10".to_string());
    let mut orders = use_signal(Vec::<OrderDto>::new);
    let mut error = use_signal(|| Option::<String>::None);

    // Poll the order list roughly every 1.5s so status transitions show live.
    use_future(move || async move {
        loop {
            match list_orders().await {
                Ok(list) => {
                    orders.set(list);
                    error.set(None);
                }
                Err(e) => error.set(Some(e.to_string())),
            }
            sleep_ms(1500).await;
        }
    });

    let create = move |_| async move {
        let amt = amount().trim().parse::<u32>().unwrap_or(0);
        match start_order(OrderInput { item: item(), amount: amt }).await {
            Ok(_) => error.set(None),
            Err(e) => error.set(Some(e.to_string())),
        }
    };

    rsx! {
        main { class: "wrap",
            header { class: "nav",
                div {
                    h1 { "MyApp" }
                    p { class: "sub", "Dioxus server functions driving a graphile_worker order pipeline." }
                }
                div { class: "row",
                    UserButton {}
                }
            }

            section { class: "card",
                h2 { "New order" }
                div { class: "row",
                    input {
                        value: "{item}",
                        oninput: move |e| item.set(e.value()),
                        placeholder: "Item",
                    }
                    input {
                        r#type: "number",
                        value: "{amount}",
                        oninput: move |e| amount.set(e.value()),
                        placeholder: "Amount",
                    }
                    button { class: "primary", onclick: create, "Create order" }
                }
            }

            if let Some(e) = error() {
                p { class: "err", "Error: {e}" }
            }

            section { class: "card",
                h2 { "Orders" }
                if orders().is_empty() {
                    p { class: "muted", "No orders yet — create one above." }
                } else {
                    table {
                        thead {
                            tr { th { "Item" } th { "Amount" } th { "Id" } th { "Status" } }
                        }
                        tbody {
                            for o in orders() {
                                tr { key: "{o.id}",
                                    td { "{o.item}" }
                                    td { "{o.amount}" }
                                    td { class: "mono", "{o.id}" }
                                    td { span { class: status_class(&o.status), "{o.status}" } }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
```

- [ ] **Step 11: server.rs — scope by the Clerk user id**

In each of `start_order`, `list_orders`, `get_order`, replace `let user_id = crate::auth::require_user_id(&state).await?;` with:

```rust
    let user_id = crate::auth::require_user_id()?;
```

Leave the rest of each fn (insert/list_for_user/get_for_user, worker `add_job`) unchanged.

- [ ] **Step 12: README.md — rewrite the auth story**

Rewrite `chapters/07-background-jobs/README.md`'s auth references to Clerk (as ch6). Keep all graphile_worker, polling, and Docker content. If the README documents the sqlx pin rationale, update it to match the Cargo.toml comment (graphile_worker + order queries share sqlx 0.8, not better-auth).

- [ ] **Step 13: Check both builds**

```bash
CLERK_PUBLISHABLE_KEY=pk_test_dummy cargo check -p ch07-background-jobs --no-default-features --features server
CLERK_PUBLISHABLE_KEY=pk_test_dummy cargo check -p ch07-background-jobs
```

- [ ] **Step 14: Commit**

```bash
git add -A chapters/07-background-jobs
git commit -m "feat(ch07): migrate background-jobs from better-auth.rs to dioxus-clerk"
```

---

### Task 5: Top-level README + workspace check

**Files:**
- Modify: `README.md`
- Check: whole workspace

- [ ] **Step 1: README.md — update prerequisites and chapter table**

In "Before you start", add a bullet:

```markdown
- **A Clerk account** — free at [clerk.com](https://clerk.com); starting in chapter 4 you'll need a publishable key and a secret key from your Clerk app
```

In the chapters table, replace the ch4 and ch5 rows with:

```markdown
| 4 | [user-accounts](chapters/04-user-accounts/README.md) | Wiring up Clerk — hosted email/password (and social) accounts with drop-in components; no local auth tables |
| 5 | [sessions](chapters/05-sessions/README.md) | Using Clerk sessions: gating pages with `SignedIn`/`SignedOut`, embedded sign-in/up, protected routes |
```

Scan the rest of the top-level README for any other "better-auth" mention and update it to Clerk.

- [ ] **Step 2: Verify no stray better-auth references remain**

```bash
git grep -i "better.auth\|better_auth\|BetterAuth" -- ':!Cargo.lock' ':!docs/'
```

Expected: no output (Cargo.lock is regenerated by cargo; the design/plan docs may mention it historically and are excluded).

- [ ] **Step 3: Workspace check + regenerate lockfile**

```bash
CLERK_PUBLISHABLE_KEY=pk_test_dummy cargo check --workspace
```

Expected: compiles, and `Cargo.lock` no longer lists `better-auth` but does list `dioxus-clerk`. If the wasm/client transitive-dependency risk from Global Constraints materializes, record the exact error in this task's commit message and in the final summary.

- [ ] **Step 4: Commit**

```bash
git add -A README.md Cargo.lock
git commit -m "docs: update top-level README for dioxus-clerk migration"
```

---

## Self-Review

**Spec coverage:** Cross-cutting changes (Cargo, .env, main, app, state, auth, drop 0002) → Tasks 1–4. Per-chapter reframing (ch4 components, ch5 gating, ch6 de-FK'd user_id, ch7 worker+pin comment) → Tasks 1–4. READMEs → each chapter task + Task 5. Verification → per-task checks + Task 5 workspace check. Out-of-scope items are not introduced. All spec sections map to tasks.

**Placeholder scan:** No TBD/TODO. Repetitive per-chapter code (auth.rs, state.rs, ClerkProvider wrap, SignIn/SignUp pages) is given in full in Task 1/2 and referenced by exact prior step; because tasks execute in order by a single implementer per chapter, the referenced code is concrete and already on the page. README rewrites are described by exact required content, not left vague.

**Type consistency:** `require_user_id()` — no args, non-async, returns `Result<String, ServerFnError>` — defined in Task 2 Step 4 and called as `require_user_id()?` in Tasks 2/3/4 server.rs. `AppState { pool }` (ch4/5/6) and `AppState { pool, worker }` (ch7) match their consumers. `ClerkProvider { publishable_key: env!("CLERK_PUBLISHABLE_KEY") }` consistent everywhere. Component names (`SignedIn`, `SignedOut`, `SignInButton`, `SignUpButton`, `UserButton`, `SignIn`, `SignUp`, `RedirectToSignIn`) match the crate's public API.
