# Chapter 5 — Sessions

## What you'll learn

How to turn "a `login` function that returns a user" into "a browser that
stays logged in": cookie-based sessions with
[`tower-sessions`](https://docs.rs/tower-sessions), a Dioxus **router** with
multiple pages, and a protected page that redirects you to `/login` if
you're not signed in.

## Run it

```bash
docker compose up -d          # Postgres 16 on localhost:5435
cp .env.example .env
dx serve
```

Register, and you'll land on the orders page showing "Signed in as ...".
Reload — you're still signed in. Sign out, and you're bounced to `/login`.
Try visiting `/` directly while signed out: same redirect. The orders list
is still global (not scoped to you) — that's next chapter.

## How it works

- **`tower_sessions_sqlx_store::PostgresStore`** stores session data in
  Postgres (its own table, migrated with `.migrate()`). `main.rs` wraps it in
  a `SessionManagerLayer` and layers it onto the axum router, right next to
  the `AppState` extension layer you already had. Every request now carries
  a session cookie.
- **`src/auth.rs`**'s `register`/`login` now take a `session:
  tower_sessions::Session` extractor and call `session.insert("user_id",
  user.id)` — that's the whole mechanism. `logout` calls `session.flush()`.
  `current_user` reads the id back out and looks up the user.
- **`require_user_id`** is the server-side auth boundary: it reads
  `user_id` from the session or fails with `"unauthenticated"`. `server.rs`'s
  `start_order` and `list_orders` both call it now — note they don't yet
  *use* the returned id for anything, they just require it to exist. That's
  intentional: this chapter is about **authentication** (are you someone?),
  chapter 6 is about **authorization/scoping** (which orders are yours?).
- **`src/app.rs`** now defines a `Route` enum with `#[route(...)]` paths and
  renders `Router::<Route> {}` instead of a single component. Each page
  moved into its own file under `src/pages/`.
- **`src/pages/orders.rs`** is *client-side* protected: on mount it calls
  `current_user()`, and if that's `None`, navigates to `/login`. This is
  purely a UX nicety — the real enforcement is `require_user_id` on the
  server. If you disabled the client redirect entirely, the API would still
  refuse unauthenticated requests.

## Your turn: get to chapter 6

Chapter 6 finally scopes orders to the logged-in user: each order gets a
`user_id`, and every query filters by it.

1. **Copy this chapter as your working copy:**

   ```bash
   cp -r ../05-sessions ../my-06-orders-per-user
   cd ../my-06-orders-per-user
   ```

2. **Add a migration**, `migrations/0003_add_user_id_to_orders.sql`:

   ```sql
   ALTER TABLE orders
       ADD COLUMN user_id UUID REFERENCES users(id) ON DELETE CASCADE;

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

3. **Update `orders.rs`** so every function takes a `user_id: Uuid` and
   uses it:

   ```rust
   #[derive(Debug, Clone, PartialEq, FromRow)]
   pub struct OrderRow {
       pub id: Uuid,
       pub user_id: Uuid,
       pub item: String,
       pub amount: i64,
       pub status: String,
   }

   pub async fn insert(pool: &PgPool, user_id: Uuid, item: &str, amount: u32) -> Result<OrderRow, sqlx::Error> {
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

   pub async fn list_for_user(pool: &PgPool, user_id: Uuid) -> Result<Vec<OrderRow>, sqlx::Error> {
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
   #[post("/api/orders/start", state: axum::Extension<crate::state::AppState>, session: tower_sessions::Session)]
   pub async fn start_order(order: OrderInput) -> ServerFnResult<String> {
       let user_id = crate::auth::require_user_id(&session).await?;
       let row = crate::orders::insert(&state.pool, user_id, &order.item, order.amount)
           .await
           .map_err(|e| ServerFnError::new(e.to_string()))?;
       Ok(row.id.to_string())
   }

   #[get("/api/orders/list", state: axum::Extension<crate::state::AppState>, session: tower_sessions::Session)]
   pub async fn list_orders() -> ServerFnResult<Vec<OrderDto>> {
       let user_id = crate::auth::require_user_id(&session).await?;
       let rows = crate::orders::list_for_user(&state.pool, user_id)
           .await
           .map_err(|e| ServerFnError::new(e.to_string()))?;
       Ok(rows.into_iter().map(dto).collect())
   }
   ```

   This one-line change per function — `let user_id = ...?;` instead of
   `crate::auth::require_user_id(&session).await?;` on its own — is the
   entire difference between "you must be logged in" (chapter 5,
   authentication) and "you can only see your own orders" (this chapter,
   authorization). Same guard call; the return value just gets used now.

5. **Try it with two accounts.** Register two different users in two
   browser profiles (or one normal + one incognito window), create orders
   in each, and confirm every account only ever sees its own list.

## Check your work

[chapters/06-orders-per-user](../06-orders-per-user) has the full version.

**Next:** [Chapter 6 — Orders per user](../06-orders-per-user/README.md)
