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

## Your turn: get to chapter 5

Chapter 5 doesn't change *how* sessions work — Clerk already handles that.
It's about *using* the session: a multi-page router, a protected orders page
that redirects to a sign-in route when signed out, and a `require_user_id`
guard reused by every protected server fn.

1. **Copy this chapter as your working copy:**

   ```bash
   cp -r ../04-user-accounts ../my-05-sessions
   cd ../my-05-sessions
   ```

2. **Add the Dioxus `router` feature** to `Cargo.toml`:

   ```toml
   dioxus = { version = "0.7", features = ["fullstack", "router"] }
   ```

3. **Split `App` into a router.** Give each surface its own file under
   `src/pages/` — `pages/login.rs` and `pages/register.rs` each render one
   Clerk widget, `pages/orders.rs` holds the protected orders UI — then in
   `app.rs` keep `ClerkProvider` at the top and put a `Router` inside it:

   ```rust
   use dioxus::prelude::*;
   use crate::pages::login::LoginPage;
   use crate::pages::orders::OrdersPage;
   use crate::pages::register::RegisterPage;

   #[derive(Routable, Clone, PartialEq)]
   pub enum Route {
       #[route("/")]
       OrdersPage {},
       #[route("/login")]
       LoginPage {},
       #[route("/register")]
       RegisterPage {},
   }

   pub fn App() -> Element {
       rsx! {
           style { {CSS} }
           dioxus_clerk::ClerkProvider { publishable_key: env!("CLERK_PUBLISHABLE_KEY"),
               Router::<Route> {}
           }
       }
   }
   ```

   `ClerkProvider` must stay above `Router` so every page can read auth
   state. Each page component is a normal `#[component]` fn; `LoginPage` /
   `RegisterPage` just render Clerk's embedded `SignIn {}` / `SignUp {}`
   widgets.

4. **Protect the orders page with Clerk gating.** In `pages/orders.rs`, let
   Clerk decide what renders:

   ```rust
   use dioxus_clerk::{RedirectToSignIn, SignedIn, SignedOut};

   #[component]
   pub fn OrdersPage() -> Element {
       rsx! {
           SignedOut { RedirectToSignIn {} }
           SignedIn { OrdersView {} }   // your real orders UI
       }
   }
   ```

   `RedirectToSignIn` navigates anonymous visitors to Clerk's sign-in flow;
   the real UI only mounts inside `SignedIn`. This client-side gating is only
   a UX nicety — the real enforcement is `require_user_id` on the server.

5. **Gate the orders server fns.** Add a tiny server-only helper in
   `auth.rs`:

   ```rust
   #[cfg(feature = "server")]
   pub fn require_user_id() -> Result<String, dioxus::prelude::ServerFnError> {
       Ok(dioxus_clerk::server::current_auth()?.user_id)
   }
   ```

   `current_auth()` reads the identity that `ClerkAuthLayer` (already wired in
   `main.rs`) verified from the session cookie. Call `require_user_id()?` as
   the first line of `start_order` and `list_orders` — for now just to gate
   access, without using the returned id yet (that's chapter 6).

## Check your work

[chapters/05-sessions](../05-sessions) has the full version.

**Next:** [Chapter 5 — Sessions](../05-sessions/README.md)
