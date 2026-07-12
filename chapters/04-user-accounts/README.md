# Chapter 4 — User accounts

## What you'll learn

How to add real accounts with [Clerk](https://clerk.com) — a hosted
authentication service — instead of hand-rolling password hashing and
sessions yourself. You'll mount a `ClerkProvider`, drop in Clerk's own
sign-in / sign-up / user-menu components, and gate UI on whether someone is
signed in. Because Clerk hosts the accounts, sign-up, password handling, and
sessions are all working by the end of this chapter with no auth tables or
server-side credential code of your own (chapter 5 is about *using* the
session Clerk gives you, not building it).

## Get your Clerk keys

1. Create a free account and an application at
   [dashboard.clerk.com](https://dashboard.clerk.com).
2. Open **API keys** and copy the **Publishable key** (`pk_test_…`) and the
   **Secret key** (`sk_test_…`).
3. Put both in `.env` (see below). The publishable key is safe to ship to the
   browser; the secret key is server-only and must never reach the WASM
   bundle.

## Run it

```bash
docker compose up -d          # Postgres 16 on localhost:5434
cp .env.example .env          # then paste your pk_test_… / sk_test_… keys
dx serve
```

Click **Create account**, sign up through Clerk's widget, and you're returned
signed in — the header swaps the buttons for a Clerk `UserButton` (avatar
menu with sign-out), and the orders section appears. Reload the page: you
stay signed in, because Clerk set a session cookie. Open your Clerk
dashboard's **Users** tab and you'll see the account you just created living
in Clerk's cloud, not in your Postgres. Orders are still unscoped from
chapter 3 — everyone signed in sees the same list.

## How it works

- **No local auth tables.** There's no `0002_better_auth.sql` and no `users`
  table: accounts, passwords, and sessions all live in Clerk. Your database
  still holds only the chapter-3 `orders` table.
- **`src/app.rs`** wraps the whole UI in
  `ClerkProvider { publishable_key: env!("CLERK_PUBLISHABLE_KEY"), … }`. The
  provider loads clerk-js in the browser and makes auth state available to
  every component below it. The publishable key is read at **build time**
  with `env!`, so it's baked into the client bundle.
- **Drop-in components do the UI.** `SignedOut { … }` renders its children
  only when nobody is signed in (here, `SignInButton` and `SignUpButton`),
  and `SignedIn { … }` renders only when someone is (here, `UserButton` plus
  the orders section). There are no hand-written forms, no `oninput`
  password handling — Clerk's widgets own all of that.
- **`src/state.rs`** shrinks to just the `PgPool`. There's no auth object to
  build and no secret to read here; the only env var this file needs is
  `DATABASE_URL`.
- **`src/main.rs`** adds `ClerkAuthLayer::from_env()` to the Axum router. It
  reads `CLERK_SECRET_KEY` at runtime and verifies the Clerk session cookie
  on every request, so that from chapter 5 on, server functions can trust the
  caller's identity. It's harmless here (this chapter has no protected server
  fn yet) but wiring it now keeps the setup identical across the rest of the
  tutorial.
- **`build.rs`** loads `.env` at *build* time. `env!("CLERK_PUBLISHABLE_KEY")`
  is a compile-time macro — it reads the environment of the `cargo`/`dx`
  process, not the `.env` file — so without this step you'd have to `export
  CLERK_PUBLISHABLE_KEY=…` in your shell before every build. The build script
  reads `.env` with [`dotenvy`](https://crates.io/crates/dotenvy) (a
  `[build-dependencies]` entry — build scripts can't see your regular
  `[dependencies]`) and re-emits each entry as `cargo:rustc-env` so `env!`
  can see it:

  ```rust
  // build.rs
  fn main() {
      // Rerun whenever .env changes (including when it's first created).
      println!("cargo:rerun-if-changed=.env");

      if let Ok(iter) = dotenvy::from_path_iter(".env") {
          for (key, value) in iter.flatten() {
              println!("cargo:rustc-env={key}={value}");
          }
      }
  }
  ```

  If `.env` is absent, `from_path_iter` returns `Err`, the block is skipped,
  and the build falls back to the real process environment — which is how CI
  and the Docker build (that passes the key as a `--build-arg`) supply it.
  This is why `cp .env.example .env` followed by `dx serve` is all you need —
  no manual `export`.

## Your turn: get to chapter 5

Chapter 5 doesn't change *how* sessions work — Clerk already handles that.
It's about *using* the session: a multi-page router, and a protected orders page
that redirects to a sign-in route when signed out.

1. **Copy this chapter as your working copy:**

   ```bash
   cp -r ../04-user-accounts ../my-05-sessions
   cd ../my-05-sessions
   ```

2. **Add the Dioxus `router` feature** in `Cargo.toml` — extend the existing
   `dioxus` line:

   ```toml
   dioxus = { version = "0.7", features = ["fullstack", "router"] } # <-- add "router"
   ```

3. **Create the pages module with the sign-in / sign-up routes.** Each surface
   gets its own file under `src/pages/`. Start with the module file,
   `pages/mod.rs`:

   ```rust
   pub mod login;
   pub mod orders;
   pub mod register;
   ```

   `pages/login.rs` is a single Clerk widget on its own route — Clerk's
   embedded `<SignIn />` renders the whole form and drops the session cookie on
   success:

   ```rust
   use dioxus::prelude::*;
   use dioxus_clerk::SignIn;

   #[component]
   pub fn Login() -> Element {
       rsx! {
           main { class: "wrap narrow",
               h1 { "Sign in" }
               p { class: "sub", "MyApp order pipeline demo" }
               SignIn {}
           }
       }
   }
   ```

   `pages/register.rs` is the mirror image — the same file with `SignUp` in
   place of `SignIn` and a "Create account" heading.

4. **Move the orders UI into `pages/orders.rs`, gated by Clerk.** First add the
   gate component — anonymous visitors get bounced to Clerk's sign-in flow, the
   real UI only mounts when signed in:

   ```rust
   use dioxus::prelude::*;
   use dioxus_clerk::{RedirectToSignIn, SignedIn, SignedOut, UserButton};
   use crate::server::{list_orders, start_order, OrderDto, OrderInput};

   #[component]
   pub fn Orders() -> Element {
       rsx! {
           SignedOut { RedirectToSignIn {} } // <-- bounce anonymous visitors to sign-in
           SignedIn { OrdersView {} }        // <-- real UI only mounts when signed in
       }
   }
   ```

   Then add `OrdersView` below it — this is chapter 4's `OrdersSection`, moved
   here almost verbatim, with the header + `UserButton` brought inside it:

   ```rust
   #[component]
   fn OrdersView() -> Element {
       // the same signals, refresh/create handlers, use_future, and rsx! as
       // chapter 4's OrdersSection — plus a `header { class: "nav" }` holding
       // the title and a `UserButton {}` for sign-out
   }
   ```

   The client-side gating is only a UX nicety — the real enforcement is
   `current_auth` on the server (step 7).

5. **Rewrite `app.rs` as a router.** Replace chapter 4's inline header + gated
   orders with a `Route` enum and a `Router`. First the imports and routes:

   ```rust
   use dioxus::prelude::*;
   use crate::pages::login::Login;
   use crate::pages::orders::Orders;
   use crate::pages::register::Register;

   #[derive(Routable, Clone, PartialEq)]
   pub enum Route {
       #[route("/")]
       Orders {},
       #[route("/login")]
       Login {},
       #[route("/register")]
       Register {},
   }
   ```

   Then shrink `App` to the shell — `ClerkProvider` stays on top so every page
   can read auth state, with the `Router` inside it:

   ```rust
   pub fn App() -> Element {
       rsx! {
           style { {CSS} }
           dioxus_clerk::ClerkProvider { publishable_key: env!("CLERK_PUBLISHABLE_KEY"),
               Router::<Route> {} // <-- replaces the old inline header + orders
           }
       }
   }
   ```

6. **Register the new modules** in `main.rs`, next to the existing `mod`
   declarations:

   ```rust
   mod app;
   mod pages; // <-- add this
   mod server;
   ```

7. **Gate the orders server fns.** Call `dioxus_clerk::server::current_auth` as
   the first line of each protected server fn in `server.rs`:

   ```rust
   pub async fn start_order(order: OrderInput) -> ServerFnResult<String> {
       dioxus_clerk::server::current_auth()?; // <-- add this
       // ...rest unchanged
   }

   pub async fn list_orders() -> ServerFnResult<Vec<OrderDto>> {
       dioxus_clerk::server::current_auth()?; // <-- add this
       // ...rest unchanged
   }
   ```

   `current_auth()` reads the identity that `ClerkAuthLayer` (already wired in
   `main.rs` back in chapter 4) verified from the session cookie.
   For now this only *gates* access — the returned id is discarded. Actually
   using it to scope orders per user is chapter 6.

## Check your work

[chapters/05-sessions](../05-sessions) has the full version.

**Next:** [Chapter 5 — Sessions](../05-sessions/README.md)
