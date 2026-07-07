# Chapter 4 — User accounts

## What you'll learn

A `users` table, and the two functions every app with accounts needs:
`register` and `login`. You'll hash passwords with
[`argon2`](https://docs.rs/argon2) so plaintext passwords never touch the
database — and you'll see *why* you need sessions (chapter 5) by feeling
their absence.

## Run it

```bash
docker compose up -d          # Postgres 16 on localhost:5434
cp .env.example .env
dx serve
```

Register a user, then reload the page. Notice the "registered as ..."
message is gone, and there's no way to tell the app who you are anymore.
That's expected — nothing persists identity across requests yet. Orders are
still unscoped from chapter 3.

## How it works

- **`migrations/0002_create_users.sql`** adds a `users` table:
  `username` is `UNIQUE` (the database enforces no duplicates even under a
  race), and `password_hash` stores an argon2 hash, never the raw password.
- **`src/auth.rs`**'s `hash_password` generates a random salt and hashes
  with `Argon2::default().hash_password(...)`. The salt is embedded in the
  returned string, so `verify_password` can re-derive it from `PasswordHash::new(hash)`.
  Hashing the same password twice gives different output (see the test) —
  that's the salt doing its job, and it's why you can't just compare hashes
  with `==`.
- **`register`** checks the username isn't taken, hashes the password, and
  inserts a row. **`login`** looks up the user and calls `verify_password`.
  Both return the *same* generic "invalid username or password" error for
  "no such user" and "wrong password" — if they differed, an attacker could
  use the error message to enumerate valid usernames.
- Neither function touches a session or a cookie. They're pure request/response
  — call them, get a result, nothing is remembered.

## Your turn: get to chapter 5

Chapter 5 adds real sessions: a cookie-backed identity that persists across
requests, a login/register/orders **router** (multiple pages), and a
protected page that redirects you to `/login` if you're not signed in. This
is a meaty chapter — the reference code is the fastest way to unblock
yourself if something doesn't click.

1. **Copy this chapter as your working copy:**

   ```bash
   cp -r ../04-user-accounts ../my-05-sessions
   cd ../my-05-sessions
   ```

2. **Add `tower-sessions` and a Postgres-backed session store**, plus
   Dioxus's `router` feature:

   ```toml
   dioxus = { version = "0.7", features = ["fullstack", "router"] }

   tower-sessions = { version = "0.14", optional = true }
   tower-sessions-sqlx-store = { version = "0.15", features = ["postgres"], optional = true }
   ```

   Add `"dep:tower-sessions"` and `"dep:tower-sessions-sqlx-store"` to the
   `server` feature list. `tower-sessions` gives you a `Session` you can
   read/write from a request; `tower-sessions-sqlx-store` is the piece that
   persists that session's data in Postgres (in its own table) instead of,
   say, only in an in-memory map that would forget everyone on restart.

3. **Layer the session middleware in `main.rs`.** This has to happen
   *after* you have a `PgPool` to give it, so it lives inside the
   `dioxus::serve` closure:

   ```rust
   #[cfg(feature = "server")]
   dioxus::serve(|| async {
       let state = state::AppState::new().await;

       let session_store = tower_sessions_sqlx_store::PostgresStore::new(state.pool.clone());
       session_store
           .migrate()
           .await
           .expect("failed to migrate session store");
       // `with_secure(false)` so the cookie works over plain http in dev;
       // set it to true behind TLS in production.
       let session_layer =
           tower_sessions::SessionManagerLayer::new(session_store).with_secure(false);

       Ok(dioxus::server::router(App)
           .layer(session_layer)
           .layer(axum::Extension(state)))
   });
   ```

   `PostgresStore::new` doesn't touch the database yet — `.migrate().await`
   is the call that actually creates its session table, separately from
   your own `sqlx::migrate!` migrations. `SessionManagerLayer` is what
   attaches a `Session` to every incoming request (creating a new one and
   setting the cookie on first visit) and saves it back to the store after
   the response is built. `.layer()` calls stack — this one sits alongside
   the `axum::Extension(state)` layer you already had, and axum runs them
   in order.

4. **Give `register` and `login` a session, and remember the user.** Add a
   `session: tower_sessions::Session` extractor argument (same mechanism as
   the `state:` extractor from chapter 3) and insert the user's id after
   success:

   ```rust
   #[cfg(feature = "server")]
   pub const SESSION_USER_KEY: &str = "user_id";

   #[post("/api/auth/register", state: axum::Extension<crate::state::AppState>, session: tower_sessions::Session)]
   pub async fn register(username: String, password: String) -> ServerFnResult<CurrentUser> {
       // ...same validation and insert as chapter 4...
       session
           .insert(SESSION_USER_KEY, user.id)
           .await
           .map_err(|e| ServerFnError::new(e.to_string()))?;
       Ok(CurrentUser { id: user.id.to_string(), username: user.username })
   }
   ```

   Do the same in `login`. `session.insert(key, value)` serializes `value`
   (here, a `uuid::Uuid`) into the session's storage under that string key —
   that's the entire mechanism. Add `logout` and `current_user` too:

   ```rust
   #[post("/api/auth/logout", session: tower_sessions::Session)]
   pub async fn logout() -> ServerFnResult<()> {
       session.flush().await.map_err(|e| ServerFnError::new(e.to_string()))?;
       Ok(())
   }

   #[get("/api/auth/me", state: axum::Extension<crate::state::AppState>, session: tower_sessions::Session)]
   pub async fn current_user() -> ServerFnResult<Option<CurrentUser>> {
       let Ok(Some(user_id)) = session.get::<uuid::Uuid>(SESSION_USER_KEY).await else {
           return Ok(None);
       };
       let user = crate::users::find_by_id(&state.pool, user_id)
           .await
           .map_err(|e| ServerFnError::new(e.to_string()))?;
       Ok(user.map(|u| CurrentUser { id: u.id.to_string(), username: u.username }))
   }
   ```

   `session.flush()` deletes the session's data entirely — that's what
   "signing out" means here, there's no separate "logged out" flag to set.
   `current_user`'s `let ... else { return Ok(None) }` collapses "no
   cookie", "cookie but no `user_id` key", and "store lookup failed" into
   one outcome: not logged in. That's a deliberate choice — an expired or
   corrupt session should look like "logged out" to the caller, not like a
   server error.

5. **Write a `require_user_id` helper** — the one place every protected
   server fn checks auth:

   ```rust
   pub const UNAUTHENTICATED: &str = "unauthenticated";

   #[cfg(feature = "server")]
   pub async fn require_user_id(session: &tower_sessions::Session) -> Result<uuid::Uuid, ServerFnError> {
       session
           .get::<uuid::Uuid>(SESSION_USER_KEY)
           .await
           .ok()
           .flatten()
           .ok_or_else(|| ServerFnError::new(UNAUTHENTICATED))
   }
   ```

   Then in `server.rs`, add a `session:` argument to `start_order` and
   `list_orders` and call `crate::auth::require_user_id(&session).await?;`
   as their first line — for now just to gate access, without yet using the
   returned id for anything (that's chapter 6). The `UNAUTHENTICATED`
   constant is public and reused verbatim in the UI in the next step — the
   client checks for this exact string to know when to redirect.

6. **Split `App` into a router.** Move each existing form into its own file
   under `src/pages/` (`pages/login.rs`, `pages/register.rs`,
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

7. **Protect the orders page.** In `pages/orders.rs`, on mount, call
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
   server call might fail with `UNAUTHENTICATED` (e.g. `list_orders`) so a
   session that expires mid-visit also redirects, not just a cold load.

   This client-side check is only ever a UX nicety, though — the real
   enforcement is `require_user_id` on the server from step 5. If you
   deleted this whole `use_future` block, unauthenticated visitors would
   see an empty/broken page instead of being redirected, but they still
   couldn't fetch anyone's data.

## Check your work

[chapters/05-sessions](../05-sessions) has the full working version.

**Next:** [Chapter 5 — Sessions](../05-sessions/README.md)
