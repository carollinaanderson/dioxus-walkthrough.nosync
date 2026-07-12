# Chapter 2 — Server functions

## What you'll learn

Dioxus's "fullstack" mode lets you write one function that's callable from
the browser but runs on your server. You'll turn on the `server` feature,
boot a real axum server, and call a `#[server]` function from a button click.

## Run it

```bash
dx serve
```

Click "Ping the server" — you should see `pong from the server 🏓` appear.
Open your browser's network tab and you'll see an actual `GET /api/ping`
request go out.

## How it works

- **Two builds, one crate.** `Cargo.toml` now has a `server` feature that
  pulls in `axum` and `tokio` (server-only, native dependencies — they don't
  compile to WASM). `dx serve` builds your crate twice: once with `web`
  (browser/WASM) and once with `server` (native binary), and wires them
  together.
- **`src/main.rs`** branches on `#[cfg(feature = "server")]`. The WASM build
  calls `dioxus::launch(App)` like chapter 1. The server build calls
  `dioxus::serve(...)`, which boots an axum router (`dioxus::server::router`)
  that serves your app *and* every `#[server]` function as an HTTP endpoint.
- **`src/server.rs`** defines `ping`, annotated `#[get("/api/ping")]`. That
  macro generates two different bodies depending on which build it's
  compiled into: on the server, the function body you wrote, wired to a real
  route; in the browser, a stub that does `fetch("/api/ping")` and decodes
  the response. Same function signature either way.
- **`src/app.rs`** calls `ping().await` from a button's `onclick`. Notice
  there's no manual JSON, no manual `fetch` — it reads like a normal async
  function call.

## Your turn: get to chapter 3

Chapter 3 replaces the "pong" string with something read from a real
Postgres database: an `orders` table you can insert into and list from.
This is the biggest jump so far — take it slow, and lean on the answer key.

1. **Copy this chapter as your working copy:**

   ```bash
   cp -r ../02-server-functions ../my-03-orders-database
   cd ../my-03-orders-database
   ```

2. **Start Postgres.** Create `docker-compose.yml`:

   ```yaml
   services:
     postgres:
       image: postgres:16
       environment:
         POSTGRES_USER: myapp
         POSTGRES_PASSWORD: myapp
         POSTGRES_DB: myapp_ch03
       ports:
         - "5433:5432"
       volumes:
         - pgdata:/var/lib/postgresql/data
   volumes:
     pgdata:
   ```

   and `.env`:

   ```
   DATABASE_URL=postgres://myapp:myapp@localhost:5433/myapp_ch03
   ```

   Bring it up with `docker compose up -d`. The port (`5433`, not Postgres's
   usual `5432`) just avoids colliding with any other Postgres you might
   already have running.

3. **Add `sqlx`, `uuid`, and `dotenvy` as server-only dependencies.** Add them
   under `[dependencies]`, one line at a time — all `optional = true`, so they
   stay out of the WASM bundle:

   ```toml
   sqlx = { version = "0.8", features = ["runtime-tokio", "postgres", "migrate", "uuid"], optional = true } # <-- add this
   uuid = { version = "1", features = ["v4", "serde"], optional = true }                                    # <-- add this
   dotenvy = { version = "0.15", optional = true }                                                          # <-- add this
   ```

   `sqlx`'s `"migrate"` feature lets it run the `.sql` files you're about to
   write; `"uuid"` teaches it to bind/read Postgres's `uuid` type as Rust's
   `uuid::Uuid`. Now wire them into the `server` feature — extend that one line
   in `[features]`:

   ```toml
   server = ["dioxus/server", "dep:axum", "dep:tokio", "dep:sqlx", "dep:uuid", "dep:dotenvy"]
   #                                                    ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ add these three
   ```

4. **Write a migration.** `sqlx` treats every `.sql` file under `migrations/`
   as one migration, run once, in filename order, tracked in a table it
   creates for itself. Create `migrations/0001_create_orders.sql`. Start with
   just the table and its primary key:

   ```sql
   CREATE TABLE IF NOT EXISTS orders (
       id UUID PRIMARY KEY DEFAULT gen_random_uuid()
   );
   ```

   Then add the business columns — the item, its amount, a status defaulting
   to `'queued'`, and a creation timestamp:

   ```sql
   CREATE TABLE IF NOT EXISTS orders (
       id         UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
       item       TEXT        NOT NULL,                  -- <-- add this
       amount     BIGINT      NOT NULL,                  -- <-- add this
       status     TEXT        NOT NULL DEFAULT 'queued', -- <-- add this
       created_at TIMESTAMPTZ NOT NULL DEFAULT now()     -- <-- add this
   );
   ```

   There's no column for *who* placed the order — there's no concept of a
   user yet. That's chapter 4.

5. **Create `src/state.rs`** to hold a connection pool. Start with the imports
   and the struct:

   ```rust
   use sqlx::postgres::PgPoolOptions;
   use sqlx::PgPool;

   #[derive(Clone)]
   pub struct AppState {
       pub pool: PgPool,
   }
   ```

   `#[derive(Clone)]` matters: axum hands a *copy* of this state to every
   request handler, and `PgPool` is cheap to clone — it's a handle to a pool
   of connections, not the connections themselves. Add an async constructor as
   an empty shell:

   ```rust
   impl AppState {
       pub async fn new() -> Self {
           // fill in next
       }
   }
   ```

   Fill it in three moves. First load `.env` and read the database URL:

   ```rust
       pub async fn new() -> Self {
           dotenvy::dotenv().ok();     // <-- add this
           let database_url =          // <-- add this
               std::env::var("DATABASE_URL").expect("DATABASE_URL must be set (see .env)");
       }
   ```

   Then open the pool:

   ```rust
           let pool = PgPoolOptions::new()   // <-- add this
               .max_connections(10)          // <-- add this
               .connect(&database_url)       // <-- add this
               .await
               .expect("failed to connect to postgres");
   ```

   Then run migrations and return the state:

   ```rust
           sqlx::migrate!("./migrations")    // <-- add this
               .run(&pool)                   // <-- add this
               .await
               .expect("failed to run migrations");

           Self { pool }                     // <-- add this
   ```

   `dotenvy::dotenv().ok()` loads `.env` into the process environment if the
   file exists (`.ok()` swallows "file not found", since in production you'd
   set `DATABASE_URL` directly instead of via a `.env` file).
   `sqlx::migrate!("./migrations")` is a macro that reads your `migrations/`
   directory *at compile time* and bakes the SQL into the binary; `.run(&pool)`
   then applies whichever migrations haven't run yet, tracked in a
   `_sqlx_migrations` table it manages.

6. **Make the pool reachable from `#[server]` functions.** In `main.rs`,
   declare the two new server-only modules at the top, next to `mod server;`:

   ```rust
   #[cfg(feature = "server")]
   mod orders; // <-- add this
   #[cfg(feature = "server")]
   mod state;  // <-- add this
   ```

   They're gated behind `#[cfg(feature = "server")]` because they use
   `sqlx`/`PgPool`, which don't exist in the WASM build — compiling them there
   would fail. Now build the state inside the server entrypoint and attach it
   to the router:

   ```rust
   #[cfg(feature = "server")]
   dioxus::serve(|| async {
       let state = state::AppState::new().await;                     // <-- add this
       Ok(dioxus::server::router(App).layer(axum::Extension(state))) // <-- was: Ok(dioxus::server::router(App))
   });
   ```

   `.layer(axum::Extension(state))` attaches `state` to the router so every
   request can pull it back out — that's what the `state:` extractor argument
   in the next step reads from.

7. **Write `src/orders.rs`** — a plain module of functions that take a
   `&PgPool` and run queries. Start with the imports and the row type:

   ```rust
   use sqlx::PgPool;
   use uuid::Uuid;

   #[derive(sqlx::FromRow)]
   pub struct OrderRow {
       pub id: Uuid,
       pub item: String,
       pub amount: i64,
       pub status: String,
   }
   ```

   `#[derive(sqlx::FromRow)]` lets `sqlx` map result columns onto these fields
   by name. Add `insert` as an empty shell:

   ```rust
   pub async fn insert(pool: &PgPool, item: &str, amount: u32) -> Result<OrderRow, sqlx::Error> {
       // fill in next
   }
   ```

   Fill it with the query:

   ```rust
   pub async fn insert(pool: &PgPool, item: &str, amount: u32) -> Result<OrderRow, sqlx::Error> {
       sqlx::query_as::<_, OrderRow>(
           "INSERT INTO orders (item, amount) VALUES ($1, $2) RETURNING id, item, amount, status",
       )
       .bind(item)          // <-- fills $1
       .bind(amount as i64) // <-- fills $2
       .fetch_one(pool)
       .await
   }
   ```

   The `_, OrderRow` type argument means "give me back rows shaped like
   `OrderRow`". `.bind(...)` fills the `$1`/`$2` placeholders in order — this
   is what keeps you safe from SQL injection, since values are sent to Postgres
   separately from the query text, never concatenated into it.

   Add `list` the same way — empty shell first:

   ```rust
   pub async fn list(pool: &PgPool) -> Result<Vec<OrderRow>, sqlx::Error> {
       // fill in next
   }
   ```

   then the query (`.fetch_all`, since it returns many rows):

   ```rust
   pub async fn list(pool: &PgPool) -> Result<Vec<OrderRow>, sqlx::Error> {
       sqlx::query_as::<_, OrderRow>(
           "SELECT id, item, amount, status FROM orders ORDER BY created_at DESC",
       )
       .fetch_all(pool) // <-- many rows, not one
       .await
   }
   ```

   This module is the *only* place SQL appears in the whole app — everything
   else calls these functions.

8. **Replace `ping` in `server.rs`** with two `#[server]` functions that
   delegate to `orders.rs`. Delete `ping`, then add the wire types —
   `OrderInput` (client → server) and `OrderDto` (server → client):

   ```rust
   use serde::{Deserialize, Serialize};

   #[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
   pub struct OrderInput {
       pub item: String,
       pub amount: u32,
   }

   #[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
   pub struct OrderDto {
       pub id: String,
       pub item: String,
       pub amount: i64,
       pub status: String,
   }
   ```

   `OrderRow` (in `orders.rs`, server-only) and `OrderDto` (here, sent over the
   wire) are deliberately separate types — you control exactly which fields a
   client can ever see, independent of what's in the database row. This matters
   more once auth arrives. Add a small server-only converter between them:

   ```rust
   #[cfg(feature = "server")]
   fn dto(row: crate::orders::OrderRow) -> OrderDto {
       OrderDto { id: row.id.to_string(), item: row.item, amount: row.amount, status: row.status }
   }
   ```

   Add `start_order` as an empty shell — note the new second argument:

   ```rust
   #[post("/api/orders/start", state: axum::Extension<crate::state::AppState>)]
   pub async fn start_order(order: OrderInput) -> ServerFnResult<String> {
       // fill in next
   }
   ```

   `state: axum::Extension<crate::state::AppState>` is an *extractor argument*:
   Dioxus's `#[get]`/`#[post]` macros let you list extra named parameters typed
   as axum extractors, pulled from the request before your body runs. This one
   hands you the `AppState` you `.layer()`ed onto the router in `main.rs`, so
   `state.pool` is available inside. Fill the body:

   ```rust
   pub async fn start_order(order: OrderInput) -> ServerFnResult<String> {
       let row = crate::orders::insert(&state.pool, &order.item, order.amount) // <-- add this
           .await
           .map_err(ServerFnError::new)?;                                     // <-- DB error -> ServerFnError
       Ok(row.id.to_string())                                                 // <-- add this
   }
   ```

   Add `list_orders` the same way — shell first:

   ```rust
   #[get("/api/orders/list", state: axum::Extension<crate::state::AppState>)]
   pub async fn list_orders() -> ServerFnResult<Vec<OrderDto>> {
       // fill in next
   }
   ```

   then the body, converting each row through `dto`:

   ```rust
   pub async fn list_orders() -> ServerFnResult<Vec<OrderDto>> {
       let rows = crate::orders::list(&state.pool)          // <-- add this
           .await
           .map_err(ServerFnError::new)?;
       Ok(rows.into_iter().map(dto).collect())              // <-- add this
   }
   ```

9. **Rebuild the UI** in `app.rs`: a form (item + amount) posting to
   `start_order`, and a table rendering whatever `list_orders` returns. Build
   it up gradually — this same shape (signals for form fields, a signal for the
   list, an `onclick` that awaits a server fn and refreshes) is what you'll
   reuse for the rest of the tutorial.

   First swap the import and set up the signals. `item`/`amount` back the two
   inputs, `orders` holds the list, `error` holds the last failure:

   ```rust
   use dioxus::prelude::*;
   use crate::server::{list_orders, start_order, OrderDto, OrderInput}; // <-- swap ping for these

   pub fn App() -> Element {
       let mut item = use_signal(|| "Widget".to_string());    // <-- add this
       let mut amount = use_signal(|| "10".to_string());      // <-- add this
       let mut orders = use_signal(Vec::<OrderDto>::new);     // <-- add this
       let mut error = use_signal(|| Option::<String>::None); // <-- add this

       rsx! {
           // built up below
       }
   }
   ```

   Add a `refresh` handler that reloads the list into the signal:

   ```rust
       let refresh = move |_| async move {
           match list_orders().await {
               Ok(list) => {
                   orders.set(list);
                   error.set(None);
               }
               Err(e) => error.set(Some(e.to_string())),
           }
       };
   ```

   Add a `create` handler: parse the amount, post the order, and reload on
   success:

   ```rust
       let create = move |_| async move {
           let amt = amount().trim().parse::<u32>().unwrap_or(0);
           match start_order(OrderInput { item: item(), amount: amt }).await {
               Ok(_) => {
                   error.set(None);
                   if let Ok(list) = list_orders().await { // <-- refetch so the new row shows
                       orders.set(list);
                   }
               }
               Err(e) => error.set(Some(e.to_string())),
           }
       };
   ```

   Load the list once when the page first renders:

   ```rust
       use_future(move || async move {
           if let Ok(list) = list_orders().await {
               orders.set(list);
           }
       });
   ```

   Now fill in `rsx!`. Start with the shell — styles, heading, subtitle:

   ```rust
       rsx! {
           style { {CSS} }
           main { class: "wrap",
               h1 { "MyApp" }
               p { class: "sub", "Chapter 3: a real orders table in Postgres." }
               // two cards go here
           }
       }
   ```

   Add the "New order" card — two inputs bound to the signals, the two
   buttons, and an error line:

   ```rust
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
                       button { class: "primary", onclick: create, "Create order" }
                       button { onclick: refresh, "Refresh" }
                   }
                   if let Some(e) = error() {
                       p { class: "err", "Error: {e}" }
                   }
               }
   ```

   Then the "Orders" card — an empty-state message, or a table looping over
   `orders()`:

   ```rust
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
   ```

   Reading `orders()` inside `rsx!` subscribes the table to that signal, so
   `refresh`/`create` calling `orders.set(...)` re-renders it automatically.

## Check your work

[chapters/03-orders-database](../03-orders-database) has the full working
version, including the `docker-compose.yml` and migration.

**Next:** [Chapter 3 — Orders database](../03-orders-database/README.md)
