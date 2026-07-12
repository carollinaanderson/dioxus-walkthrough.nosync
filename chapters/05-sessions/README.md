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
  This is purely a UX nicety — the real enforcement is
  `dioxus_clerk::server::current_auth` in `auth.rs`, which `server.rs`'s
  `start_order` and `list_orders` both call. Note they don't yet *use* the
  returned id, they just require it to exist.
  That's intentional: this chapter is about **authentication** (are you
  someone?), chapter 6 is about **authorization/scoping** (which orders are
  yours?).
- **`build.rs`** loads `.env` at build time with
  [`dotenvy`](https://crates.io/crates/dotenvy) so
  `env!("CLERK_PUBLISHABLE_KEY")` resolves from your `.env` without a manual
  `export` (see [chapter 4](../04-user-accounts/README.md) for the full
  explanation). Copying this chapter forward carries the script along.

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
   is no local `users` table to reference (accounts live in Clerk), so this is
   a plain column, not a foreign key. Build it up one statement at a time.
   First add the column, nullable:

   ```sql
   ALTER TABLE orders ADD COLUMN user_id TEXT;
   ```

   Then tighten it to `NOT NULL`:

   ```sql
   -- This is a fresh tutorial database, so `orders` is empty here — no rows to
   -- backfill. In a real app with existing data you'd backfill user_id on
   -- existing rows before adding the NOT NULL constraint.
   ALTER TABLE orders ALTER COLUMN user_id SET NOT NULL; -- <-- add this
   ```

   Finally add an index matching how you'll query:

   ```sql
   CREATE INDEX IF NOT EXISTS orders_user_created_idx ON orders (user_id, created_at DESC); -- <-- add this
   ```

   Why two `ALTER` statements instead of one `NOT NULL` column from the start?
   On a table that already has rows, adding a `NOT NULL` column in one step
   fails immediately — Postgres has no value to put in existing rows. Add it
   nullable, backfill, *then* tighten. Our table is empty so it doesn't matter
   here, but this is the shape you'd use against live data. The index exists
   because `list_for_user` (next step) filters and sorts by exactly
   `(user_id, created_at DESC)` on every call.

3. **Update `orders.rs`** so every function is scoped to a `user_id`. First
   the row type — bring `FromRow` into scope, add richer derives, and add the
   `user_id` field:

   ```rust
   use sqlx::{prelude::FromRow, PgPool}; // <-- was: use sqlx::PgPool;
   use uuid::Uuid;

   #[derive(Debug, Clone, PartialEq, FromRow)] // <-- richer derives
   pub struct OrderRow {
       pub id: Uuid,
       pub user_id: String, // <-- add this
       pub item: String,
       pub amount: i64,
       pub status: String,
   }
   ```

   Give `insert` a `user_id` parameter and thread it through the query:

   ```rust
   pub async fn insert(pool: &PgPool, user_id: &str, item: &str, amount: u32) -> Result<OrderRow, sqlx::Error> {
       sqlx::query_as::<_, OrderRow>(
           "INSERT INTO orders (user_id, item, amount) VALUES ($1, $2, $3)
            RETURNING id, user_id, item, amount, status",
       )
       .bind(user_id) // <-- fills the new $1
       .bind(item)
       .bind(amount as i64)
       .fetch_one(pool)
       .await
   }
   ```

   Rename `list` to `list_for_user` and filter by the id — the name itself is
   now documentation of the contract:

   ```rust
   pub async fn list_for_user(pool: &PgPool, user_id: &str) -> Result<Vec<OrderRow>, sqlx::Error> {
       sqlx::query_as::<_, OrderRow>(
           "SELECT id, user_id, item, amount, status FROM orders
            WHERE user_id = $1 ORDER BY created_at DESC", // <-- WHERE user_id
       )
       .bind(user_id)
       .fetch_all(pool)
       .await
   }
   ```

   There is no `list()` returning everyone's orders anymore — that code path
   simply doesn't exist, which is a stronger guarantee than "we remember to
   filter every time." Finally add a scoped single-order lookup — chapter 6's
   `get_order` endpoint needs it. Empty shell first:

   ```rust
   pub async fn get_for_user(pool: &PgPool, user_id: &str, id: Uuid) -> Result<Option<OrderRow>, sqlx::Error> {
       // fill in next
   }
   ```

   then the query — note `user_id` is in the `WHERE`, so asking for someone
   else's order id simply returns `None`:

   ```rust
   pub async fn get_for_user(pool: &PgPool, user_id: &str, id: Uuid) -> Result<Option<OrderRow>, sqlx::Error> {
       sqlx::query_as::<_, OrderRow>(
           "SELECT id, user_id, item, amount, status FROM orders WHERE id = $1 AND user_id = $2",
       )
       .bind(id)
       .bind(user_id)
       .fetch_optional(pool) // <-- Option: "not found" is ordinary, not an error
       .await
   }
   ```

4. **Update `server.rs`** to *use* the id `dioxus_clerk::server::current_auth`
  returns instead of discarding it. In `start_order`, capture it and pass it to
  `insert`:

   ```rust
   pub async fn start_order(order: OrderInput) -> ServerFnResult<String> {
       let user_id = dioxus_clerk::server::current_auth()?.user_id;                      // <-- was: dioxus_clerk::server::current_auth()?;
       let row = crate::orders::insert(&state.pool, &user_id, &order.item, order.amount) // <-- pass user_id
           .await
           .map_err(ServerFnError::new)?;
       Ok(row.id.to_string())
   }
   ```

   Same one-line shift in `list_orders`, now calling `list_for_user`:

   ```rust
   pub async fn list_orders() -> ServerFnResult<Vec<OrderDto>> {
       let user_id = dioxus_clerk::server::current_auth()?.user_id;   // <-- capture the id
       let rows = crate::orders::list_for_user(&state.pool, &user_id) // <-- was: list(&state.pool)
           .await
           .map_err(ServerFnError::new)?;
       Ok(rows.into_iter().map(dto).collect())
   }
   ```

   Capturing the id instead of discarding it is the entire difference between
   "you must be logged in" (authentication) and "you only see your own orders"
   (authorization). Finally add a scoped single-order endpoint — shell first:

   ```rust
   #[get("/api/orders/{id}", state: axum::Extension<crate::state::AppState>)]
   pub async fn get_order(id: String) -> ServerFnResult<OrderDto> {
       // fill in next
   }
   ```

   then the body — parse the id, look it up *for this user*, and 404 if it
   isn't theirs:

   ```rust
   pub async fn get_order(id: String) -> ServerFnResult<OrderDto> {
       let user_id = dioxus_clerk::server::current_auth()?.user_id;
       let order_id = id
           .parse::<uuid::Uuid>()
           .map_err(ServerFnError::new)?;
       let row = crate::orders::get_for_user(&state.pool, &user_id, order_id)
           .await
           .map_err(ServerFnError::new)?
           .ok_or_else(|| ServerFnError::new("order not found"))?; // <-- someone else's id -> not found
       Ok(dto(row))
   }
   ```

5. **Try it with two accounts.** Register two different users in two browser
   profiles (or one normal + one incognito window), create orders in each, and
   confirm every account only ever sees its own list.

## Check your work

[chapters/06-orders-per-user](../06-orders-per-user) has the full version.

**Next:** [Chapter 6 — Orders per user](../06-orders-per-user/README.md)
