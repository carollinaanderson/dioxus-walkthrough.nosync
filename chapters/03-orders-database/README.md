# Chapter 3 — Orders database

## What you'll learn

Real apps need real storage. Here you'll wire up Postgres with
[`sqlx`](https://github.com/launchbadge/sqlx), write your first migration,
and build server functions that read and write a table.

## Run it

```bash
docker compose up -d          # Postgres 16 on localhost:5433
cp .env.example .env
dx serve
```

Create an order, hit "Refresh" (or just reload), and see it listed. Every
order is visible to everyone — there's no concept of a user yet.

## How it works

- **`docker-compose.yml`** runs Postgres in a container so you don't have to
  install it locally. This chapter uses db name `myapp_ch03` on port
  `5433` — every chapter from here on gets its own port and db name so you
  can leave several running at once without them colliding.
- **`migrations/0001_create_orders.sql`** is a plain SQL file. `sqlx` finds
  every file in `migrations/`, runs the ones it hasn't seen yet (tracked in
  a `_sqlx_migrations` table it manages), in filename order.
- **`src/state.rs`** connects a `PgPool` (a connection pool) on server boot
  and runs migrations. `AppState` holding that pool gets attached to the
  axum router with `.layer(axum::Extension(state))` in `main.rs`.
- **`src/orders.rs`** is a plain module of `async fn`s that take `&PgPool`
  and run queries with `sqlx::query_as`. This is the only place SQL appears
  — everything else calls these functions.
- **`src/server.rs`**'s `#[server]` functions pull the pool out of state with
  the `state: axum::Extension<crate::state::AppState>` extractor argument
  (this is dioxus fullstack's way of giving a server fn access to
  request-scoped data), then delegate to `orders.rs`.

## Your turn: get to chapter 4

Chapter 4 adds real accounts — but instead of hand-rolling password hashing
and a `users` table, you'll hand auth to [Clerk](https://clerk.com), a hosted
authentication service. You mount a `ClerkProvider`, drop in Clerk's own
sign-in / sign-up / user-menu widgets, and gate the UI on whether someone is
signed in. There's no auth migration, no `users` table, and no credential code
of your own — accounts and sessions live in Clerk's cloud.

1. **Copy this chapter as your working copy:**

   ```bash
   cp -r ../03-orders-database ../my-04-user-accounts
   cd ../my-04-user-accounts
   ```

2. **Get your Clerk keys.** Create a free app at
   [dashboard.clerk.com](https://dashboard.clerk.com), open **API keys**, and
   copy the **Publishable key** (`pk_test_…`) and **Secret key** (`sk_test_…`).
   The publishable key is safe to ship to the browser; the secret key is
   server-only and must never reach the WASM bundle.

3. **Point this chapter at its own database and add the Clerk keys.** Bump the
   port and db name in `docker-compose.yml` (`5434` / `myapp_ch04`) so it
   doesn't collide with a chapter-3 Postgres you might still have running, then
   replace `.env.example` (and your `.env`) with:

   ```
   DATABASE_URL=postgres://myapp:myapp@localhost:5434/myapp_ch04
   CLERK_PUBLISHABLE_KEY=pk_test_replace_me # <-- add this
   CLERK_SECRET_KEY=sk_test_replace_me      # <-- add this
   ```

4. **Add the Clerk dependencies.** Under `[dependencies]`, add two crates —
   `serde_json` (Clerk's components need it) and `dioxus-clerk` itself. Unlike
   the server-only crates, these aren't `optional`; the client needs them too:

   ```toml
   serde_json = "1"     # <-- add this
   dioxus-clerk = "0.1" # <-- add this
   ```

   Extend the `server` feature so Clerk's server-side half compiles into the
   native binary:

   ```toml
   server = ["dioxus/server", "dep:axum", "dep:tokio", "dep:sqlx", "dep:uuid", "dep:dotenvy", "dioxus-clerk/server"]
   #                                                                                          ^^^^^^^^^^^^^^^^^^^^^ add this
   ```

   Finally add a `[build-dependencies]` section — the build script in the next
   step needs `dotenvy`, and build scripts can't see your regular
   `[dependencies]`:

   ```toml
   [build-dependencies] # <-- add this section
   dotenvy = "0.15"
   ```

5. **Add a `build.rs` that loads `.env` at build time.** The publishable key
   is read with `env!("CLERK_PUBLISHABLE_KEY")` — a *compile-time* macro that
   reads the environment of the `cargo`/`dx` process, not your `.env` file. The
   build script bridges that gap so you don't have to `export` the key before
   every build. Create `build.rs` at the crate root with an empty `main`:

   ```rust
   fn main() {
       // fill in next
   }
   ```

   Tell Cargo to rerun the script whenever `.env` changes:

   ```rust
   fn main() {
       // Rerun whenever .env changes (including when it's first created).
       println!("cargo:rerun-if-changed=.env"); // <-- add this
   }
   ```

   Then read each `.env` entry and re-emit it as a `rustc-env` var so `env!`
   can see it:

   ```rust
       if let Ok(iter) = dotenvy::from_path_iter(".env") {       // <-- add this
           for (key, value) in iter.flatten() {                 // <-- add this
               println!("cargo:rustc-env={key}={value}");       // <-- add this
           }
       }
   ```

   If `.env` is absent, `from_path_iter` returns `Err`, the block is skipped,
   and the build falls back to the real process environment (how CI and the
   Docker build supply the key). So `cp .env.example .env` then `dx serve` is
   all you need — no manual `export`.

6. **Verify the Clerk session on the server.** In `main.rs`'s server branch,
   build a `ClerkAuthLayer` and add it to the router. Right now the branch is:

   ```rust
   #[cfg(feature = "server")]
   dioxus::serve(|| async {
       let state = state::AppState::new().await;
       Ok(dioxus::server::router(App).layer(axum::Extension(state)))
   });
   ```

   Build the layer from the secret key in the environment:

   ```rust
       let state = state::AppState::new().await;
       let clerk = dioxus_clerk::server::ClerkAuthLayer::from_env()          // <-- add this
           .expect("CLERK_SECRET_KEY must be set (see .env)");               // <-- add this
   ```

   Then attach it alongside the state:

   ```rust
       Ok(dioxus::server::router(App)
           .layer(clerk)                    // <-- add this
           .layer(axum::Extension(state)))
   ```

   `ClerkAuthLayer` verifies the Clerk session cookie on every request, so
   from chapter 5 on, server functions can trust the caller's identity. It's
   harmless here (this chapter has no protected server fn yet), but wiring it
   now keeps the setup identical across the rest of the tutorial. Your
   `orders.rs` and `server.rs` stay exactly as they were — orders are still
   global this chapter.

7. **Move the orders UI into its own component.** In `app.rs`, everything
   currently inside `App` — the four signals, the `refresh`/`create` handlers,
   the `use_future`, and the two `section { class: "card" }` blocks — moves
   into a new `#[component]` so it can be shown only to signed-in users. Cut it
   into `OrdersSection` (the handlers and error signal are renamed for clarity
   now that auth pieces sit alongside them):

   ```rust
   #[component]
   fn OrdersSection() -> Element {
       let mut item = use_signal(|| "Widget".to_string());
       let mut amount = use_signal(|| "10".to_string());
       let mut orders = use_signal(Vec::<OrderDto>::new);
       let mut order_error = use_signal(|| Option::<String>::None); // <-- was `error`

       let refresh_orders = move |_| async move { /* the old `refresh` body */ };
       let create_order = move |_| async move { /* the old `create` body */ };

       use_future(move || async move {
           if let Ok(list) = list_orders().await {
               orders.set(list);
           }
       });

       rsx! {
           // the two `section { class: "card" }` blocks, unchanged
           // (wire the buttons to `create_order` / `refresh_orders`)
       }
   }
   ```

8. **Rebuild `App` as a Clerk shell.** First widen the import to pull in
   Clerk's components:

   ```rust
   use dioxus::prelude::*;
   use dioxus_clerk::{ClerkProvider, SignInButton, SignUpButton, SignedIn, SignedOut, UserButton}; // <-- add this
   use crate::server::{list_orders, start_order, OrderDto, OrderInput};
   ```

   Wrap the page in a `ClerkProvider`. The publishable key is baked in at build
   time with `env!`, and the provider makes auth state available to every
   component below it:

   ```rust
   pub fn App() -> Element {
       rsx! {
           style { {CSS} }
           ClerkProvider { publishable_key: env!("CLERK_PUBLISHABLE_KEY"),
               main { class: "wrap",
                   // header + gated content below
               }
           }
       }
   }
   ```

   Add a header whose right side flips on auth state — sign-in / sign-up
   buttons when signed out, Clerk's `UserButton` (avatar menu with sign-out)
   when signed in:

   ```rust
                   header { class: "nav",
                       div {
                           h1 { "MyApp" }
                           p { class: "sub", "Chapter 4: user accounts via Clerk." }
                       }
                       div { class: "row",
                           SignedOut {                                  // <-- rendered only when signed out
                               SignInButton { class: "primary", "Sign in" }
                               SignUpButton { class: "ghost", "Create account" }
                           }
                           SignedIn { UserButton {} }                   // <-- rendered only when signed in
                       }
                   }
   ```

   Then gate the body: a prompt when signed out, the orders UI when signed in:

   ```rust
                   SignedOut {                                          // <-- add this
                       section { class: "card",
                           p { class: "muted", "Sign in or create an account to place orders." }
                       }
                   }
                   SignedIn { OrdersSection {} }                        // <-- your moved orders UI
   ```

   `SignedOut`/`SignedIn` render their children based on Clerk's resolved auth
   state; there are no hand-written forms and no password handling — Clerk's
   widgets own all of it. Run it, sign up through Clerk's widget, and the
   header swaps to the `UserButton` while the orders section appears. Orders
   are still global (everyone signed in sees the same list) — chapter 6 scopes
   them per user.

## Check your work

[chapters/04-user-accounts](../04-user-accounts) has the full version.

**Next:** [Chapter 4 — User accounts](../04-user-accounts/README.md)
