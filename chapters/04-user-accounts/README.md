# Chapter 4 — User accounts

## What you'll learn

How to wire up [better-auth.rs](https://better-auth.rs) — a real
authentication library — instead of hand-rolling password hashing and
sessions yourself: its plugin model (`EmailPasswordPlugin` +
`SessionManagementPlugin`), its own Postgres-backed tables, and a
`register`/`login` pair of `#[server]` fns that call into it. Unlike a
from-scratch implementation, sessions are already fully working by the end
of this chapter — that's inherent to using the library, not a separate
lesson (chapter 5 is about *using* that session, not building it).

## Run it

```bash
docker compose up -d          # Postgres 16 on localhost:5434
cp .env.example .env
dx serve
```

Register a user with an email and password. Reload the page — the
"registered as ..." message disappears (nothing in `app.rs` remembers it
across a reload yet), but if you check your browser's cookies for
`localhost:8080`, you'll see a session cookie already sitting there. That's
the difference from a hand-rolled version: the session exists from the very
first successful register/login call, whether or not the UI does anything
with it. Orders are still unscoped from chapter 3.

## How it works

- **`migrations/0002_better_auth.sql`** creates better-auth.rs's own
  `users`, `sessions`, `accounts`, and `verifications` tables — its
  documented schema. Notice ids are `TEXT`, not `UUID`: that's a
  better-auth.rs convention, not a choice we made.
- **`src/state.rs`** builds one `BetterAuth<SqlxAdapter>` per process,
  sharing the same `PgPool` your own migrations already ran against
  (`SqlxAdapter::from_pool(pool.clone())`). `AuthConfig::new(secret)`
  requires a 32+ character secret — read from the new `BETTER_AUTH_SECRET`
  env var. Two plugins compose the behavior: `EmailPasswordPlugin` (with
  signup enabled and an 8-character password minimum) does the actual
  hashing and credential checks; `SessionManagementPlugin` issues and
  validates the session token/cookie.
- **`src/auth.rs`** doesn't call better-auth's own Axum router (which
  would mean the frontend calling `/auth/sign-up/email` etc. directly via
  `fetch`). Instead it keeps this tutorial's `#[server]` fn pattern:
  `register`/`login` build a JSON body and call
  `auth.handle_request(AuthRequest::from_parts(method, path, headers, body,
  query))` — the same programmatic entry point better-auth's own HTTP
  router uses internally — for `/sign-up/email` / `/sign-in/email`. The
  response carries a `Set-Cookie` header, which gets forwarded to the
  browser via `FullstackContext::current().add_response_header(...)`; the
  browser then sends that cookie back on every subsequent request, which
  `auth.rs` reads (`FullstackContext::extract::<axum::http::HeaderMap,
  _>()`) and forwards *into* `handle_request` so better-auth can find the
  session again.
- **`register`/`login` both return the same shape**, `CurrentUser { id,
  email }`, parsed out of better-auth's JSON response body — there's no
  local `users` table query left in this codebase at all.

## Your turn: get to chapter 5

Chapter 5 doesn't add anything new to *how* sessions work — that's already
done. It's about *using* the session that's already there: a multi-page
router, a protected orders page that redirects to `/login` when signed out,
and a `require_user_id` guard reused by every protected server fn.

1. **Copy this chapter as your working copy:**

   ```bash
   cp -r ../04-user-accounts ../my-05-sessions
   cd ../my-05-sessions
   ```

2. **Add the Dioxus `router` feature** to `Cargo.toml`:

   ```toml
   dioxus = { version = "0.7", features = ["fullstack", "router"] }
   ```

3. **Split `App` into a router.** Move each existing form into its own
   file under `src/pages/` (`pages/login.rs`, `pages/register.rs`,
   `pages/orders.rs`), then in `app.rs`:

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
           Router::<Route> {}
       }
   }
   ```

   `#[derive(Routable)]` generates the URL-matching logic from the
   `#[route(...)]` attributes; `Router::<Route> {}` renders whichever
   variant matches the current URL. Each page component (`LoginPage`,
   `RegisterPage`, `OrdersPage`) is just a normal `#[component]` fn — the
   router's only job is picking which one to mount.

4. **Protect the orders page.** In `pages/orders.rs`, on mount, call
   `current_user()`; if it comes back `None`, navigate away:

   ```rust
   let nav = use_navigator();
   use_future(move || async move {
       match current_user().await {
           Ok(Some(u)) => user.set(Some(u)),
           Ok(None) => { nav.push(Route::LoginPage {}); }
           Err(e) => error.set(Some(e.to_string())),
       }
   });
   ```

   `use_navigator()` gives you a handle to push new routes programmatically
   (as opposed to `Link { to: ... }`, which renders a clickable link).
   `use_future` runs its async block once when the component first mounts —
   perfect for an on-load check like this. Do the same check wherever a
   server call might fail with `UNAUTHENTICATED` (e.g. `list_orders`), so a
   session that expires mid-visit also redirects, not just a cold load.

   This client-side check is only ever a UX nicety, though — the real
   enforcement is `require_user_id` (already in `auth.rs`) on the server.
   If you deleted this whole `use_future` block, unauthenticated visitors
   would see an empty/broken page instead of being redirected, but they
   still couldn't fetch anyone's data.

5. **Gate the orders server fns.** In `server.rs`, call
   `crate::auth::require_user_id(&state).await?;` as the first line of
   `start_order` and `list_orders` — for now just to gate access, without
   using the returned id for anything yet (that's chapter 6). The
   `UNAUTHENTICATED` constant is public and reused verbatim in the UI in
   the previous step — the client checks for this exact string to know
   when to redirect.

## Check your work

[chapters/05-sessions](../05-sessions) has the full version.

**Next:** [Chapter 5 — Sessions](../05-sessions/README.md)
