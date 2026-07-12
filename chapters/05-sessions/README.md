# Chapter 5 — Sessions

## What you'll learn

How to *use* the session Clerk set up in chapter 4: a Dioxus **router** with
multiple pages, embedded Clerk sign-in / sign-up widgets on their own routes,
and a protected page that redirects you to `/login` if you're not signed in.

## Run it

```bash
docker compose up -d          # Postgres 16 on localhost:5435
cp .env.example .env          # paste your pk_test_… / sk_test_… keys
dx serve
```

Visit `/register`, sign up through Clerk's widget, and you'll land on the
orders page with a Clerk `UserButton` in the header. Reload — you're still
signed in. Sign out from the user menu, and you're bounced to `/login`. Try
visiting `/` directly while signed out: same redirect. The orders list is
still global (not scoped to you) — that's next chapter.

## How it works

- **`src/app.rs`** defines a `Route` enum with `#[route(...)]` paths and
  renders `Router::<Route> {}`. Crucially, `ClerkProvider` wraps the router,
  so every page can read auth state:

  ```rust
  dioxus_clerk::ClerkProvider { publishable_key: env!("CLERK_PUBLISHABLE_KEY"),
      Router::<Route> {}
  }
  ```

- **`src/pages/login.rs` and `register.rs`** are one line of UI each: Clerk's
  embedded `SignIn {}` and `SignUp {}` widgets. Clerk renders the whole form,
  handles validation and errors, and drops the session cookie on success —
  you write none of it.
- **`src/pages/orders.rs`** is protected by Clerk's gating components rather
  than a hand-written guard:

  ```rust
  rsx! {
      SignedOut { RedirectToSignIn {} }
      SignedIn { OrdersView {} }
  }
  ```

  `SignedOut`/`SignedIn` pick a branch from resolved auth state;
  `RedirectToSignIn` navigates anonymous visitors to Clerk's sign-in flow.
  This is purely a UX nicety — the real enforcement is `require_user_id` in
  `auth.rs`, which `server.rs`'s `start_order` and `list_orders` both call.
  Note they don't yet *use* the returned id, they just require it to exist.
  That's intentional: this chapter is about **authentication** (are you
  someone?), chapter 6 is about **authorization/scoping** (which orders are
  yours?).
- **`src/auth.rs`** shrank to a single server-only function:

  ```rust
  #[cfg(feature = "server")]
  pub fn require_user_id() -> Result<String, dioxus::prelude::ServerFnError> {
      Ok(dioxus_clerk::server::current_auth()?.user_id)
  }
  ```

  `current_auth()` reads the identity that `ClerkAuthLayer` (wired in
  `main.rs`) verified from the session cookie. No `&state`, no `.await`, no
  cookie plumbing — the middleware already did that work before the server fn
  runs.

## Your turn: get to chapter 6

Chapter 6 finally scopes orders to the logged-in user: each order gets a
`user_id`, and every query filters by it.

1. **Copy this chapter as your working copy:**

   ```bash
   cp -r ../05-sessions ../my-06-orders-per-user
   cd ../my-06-orders-per-user
   ```

2. **Add a migration**, `migrations/0002_add_user_id_to_orders.sql`. The
   column type is `TEXT` — it holds a Clerk user id like `user_2abc…`. There
   is no local `users` table to reference (accounts live in Clerk), so this
   is a plain column, not a foreign key:

   ```sql
   ALTER TABLE orders ADD COLUMN user_id TEXT;

   -- This is a fresh tutorial database, so `orders` is empty here — no rows
   -- to backfill. In a real app with existing data you'd backfill user_id
   -- on existing rows before adding the NOT NULL constraint.
   ALTER TABLE orders ALTER COLUMN user_id SET NOT NULL;

   CREATE INDEX IF NOT EXISTS orders_user_created_idx ON orders (user_id, created_at DESC);
   ```

   Why two `ALTER` statements instead of one `NOT NULL` column from the
   start? On a table that already has rows, adding a `NOT NULL` column in
   one step fails immediately — Postgres has no value to put in existing
   rows. Add it nullable, backfill every existing row, *then* tighten the
   constraint. Our table is empty so it doesn't matter here, but this is
   the shape you'd actually use against live data. The index exists because
   `list_for_user` (next step) is about to filter and sort by exactly
   `(user_id, created_at DESC)` on every single call.

3. **Update `orders.rs`** so every function takes a `user_id: &str` and
   uses it:

   ```rust
   #[derive(Debug, Clone, PartialEq, FromRow)]
   pub struct OrderRow {
       pub id: Uuid,
       pub user_id: String,
       pub item: String,
       pub amount: i64,
       pub status: String,
   }

   pub async fn insert(pool: &PgPool, user_id: &str, item: &str, amount: u32) -> Result<OrderRow, sqlx::Error> {
       sqlx::query_as::<_, OrderRow>(
           "INSERT INTO orders (user_id, item, amount) VALUES ($1, $2, $3)
            RETURNING id, user_id, item, amount, status",
       )
       .bind(user_id)
       .bind(item)
       .bind(amount as i64)
       .fetch_one(pool)
       .await
   }

   pub async fn list_for_user(pool: &PgPool, user_id: &str) -> Result<Vec<OrderRow>, sqlx::Error> {
       sqlx::query_as::<_, OrderRow>(
           "SELECT id, user_id, item, amount, status FROM orders
            WHERE user_id = $1 ORDER BY created_at DESC",
       )
       .bind(user_id)
       .fetch_all(pool)
       .await
   }
   ```

   Rename `list` to `list_for_user` — the name itself is now documentation
   of the contract: you cannot call this function without saying whose
   orders you want. There is no `list()` that returns everyone's orders
   anymore; that code path simply doesn't exist, which is a stronger
   guarantee than "we remember to filter every time we call it."

4. **Update `server.rs`**: you already call `require_user_id` in chapter 5
   — now *use* what it returns instead of discarding it:

   ```rust
   #[post("/api/orders/start", state: axum::Extension<crate::state::AppState>)]
   pub async fn start_order(order: OrderInput) -> ServerFnResult<String> {
       let user_id = crate::auth::require_user_id()?;
       let row = crate::orders::insert(&state.pool, &user_id, &order.item, order.amount)
           .await
           .map_err(|e| ServerFnError::new(e.to_string()))?;
       Ok(row.id.to_string())
   }

   #[get("/api/orders/list", state: axum::Extension<crate::state::AppState>)]
   pub async fn list_orders() -> ServerFnResult<Vec<OrderDto>> {
       let user_id = crate::auth::require_user_id()?;
       let rows = crate::orders::list_for_user(&state.pool, &user_id)
           .await
           .map_err(|e| ServerFnError::new(e.to_string()))?;
       Ok(rows.into_iter().map(dto).collect())
   }
   ```

   This one-line change per function — `let user_id = ...?;` instead of
   `crate::auth::require_user_id()?;` on its own — is the entire difference
   between "you must be logged in" (chapter 5, authentication) and "you can
   only see your own orders" (this chapter, authorization). Same guard call;
   the return value just gets used now, and it's the Clerk user id.

5. **Try it with two accounts.** Register two different users in two
   browser profiles (or one normal + one incognito window), create orders
   in each, and confirm every account only ever sees its own list.

## Check your work

[chapters/06-orders-per-user](../06-orders-per-user) has the full version.

**Next:** [Chapter 6 — Orders per user](../06-orders-per-user/README.md)
