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

3. **Add `sqlx`, `uuid`, and `dotenvy` as server-only dependencies:**

   ```toml
   sqlx = { version = "0.8", features = ["runtime-tokio", "postgres", "migrate", "uuid"], optional = true }
   uuid = { version = "1", features = ["v4", "serde"], optional = true }
   dotenvy = { version = "0.15", optional = true }
   ```

   `sqlx`'s `"migrate"` feature is what lets it run the `.sql` files you're
   about to write; `"uuid"` teaches it to bind/read Postgres's `uuid` type
   as Rust's `uuid::Uuid`. Add `"dep:sqlx"`, `"dep:uuid"`, `"dep:dotenvy"`
   to the `server` feature list in `[features]`.

4. **Write a migration.** `sqlx` treats every `.sql` file under
   `migrations/` as one migration, run once, in filename order, tracked in
   a table it creates for itself. Create `migrations/0001_create_orders.sql`:

   ```sql
   CREATE TABLE IF NOT EXISTS orders (
       id         UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
       item       TEXT        NOT NULL,
       amount     BIGINT      NOT NULL,
       status     TEXT        NOT NULL DEFAULT 'queued',
       created_at TIMESTAMPTZ NOT NULL DEFAULT now()
   );
   ```

   There's no column for *who* placed the order — there's no concept of a
   user yet. That's chapter 4.

5. **Create `src/state.rs`** to hold a connection pool, and connect it on
   boot:

   ```rust
   use sqlx::postgres::PgPoolOptions;
   use sqlx::PgPool;

   #[derive(Clone)]
   pub struct AppState {
       pub pool: PgPool,
   }

   impl AppState {
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

   `#[derive(Clone)]` matters: axum hands a *copy* of this state to every
   request handler, and `sqlx::PgPool` is cheap to clone — it's a handle to
   a pool of connections, not the connections themselves.
   `dotenvy::dotenv().ok()` loads `.env` into the process's environment
   variables if the file exists (the `.ok()` swallows the "file not found"
   case, since in production you'd set `DATABASE_URL` directly instead of
   via a `.env` file). `sqlx::migrate!("./migrations")` is a macro that
   reads your `migrations/` directory *at compile time* and bakes the SQL
   into the binary — `.run(&pool)` then applies whichever migrations
   haven't run yet, tracked in a `_sqlx_migrations` table it manages.

6. **Make the pool reachable from `#[server]` functions.** Update
   `main.rs`'s server branch:

   ```rust
   #[cfg(feature = "server")]
   mod orders;
   #[cfg(feature = "server")]
   mod state;

   #[cfg(feature = "server")]
   dioxus::serve(|| async {
       let state = state::AppState::new().await;
       Ok(dioxus::server::router(App).layer(axum::Extension(state)))
   });
   ```

   `.layer(axum::Extension(state))` attaches `state` to the router so every
   request can pull it out. `mod orders`/`mod state` are gated behind
   `#[cfg(feature = "server")]` at the top of `main.rs` because these
   modules use `sqlx`/`PgPool`, which don't exist in the WASM build — trying
   to compile them there would fail.

7. **Write `src/orders.rs`**, a plain module of functions that take
   `&PgPool` and run queries:

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

   pub async fn insert(pool: &PgPool, item: &str, amount: u32) -> Result<OrderRow, sqlx::Error> {
       sqlx::query_as::<_, OrderRow>(
           "INSERT INTO orders (item, amount) VALUES ($1, $2) RETURNING id, item, amount, status",
       )
       .bind(item)
       .bind(amount as i64)
       .fetch_one(pool)
       .await
   }

   pub async fn list(pool: &PgPool) -> Result<Vec<OrderRow>, sqlx::Error> {
       sqlx::query_as::<_, OrderRow>(
           "SELECT id, item, amount, status FROM orders ORDER BY created_at DESC",
       )
       .fetch_all(pool)
       .await
   }
   ```

   `#[derive(sqlx::FromRow)]` lets `sqlx::query_as::<_, OrderRow>(...)` map
   result columns onto `OrderRow`'s fields by name — that's the `_,
   OrderRow` type argument: "give me back rows shaped like `OrderRow`".
   `.bind(...)` fills in the `$1`, `$2` placeholders, in order — this is
   what keeps you safe from SQL injection, since values are sent to Postgres
   separately from the query text, never concatenated into it. This module
   is the *only* place SQL appears in the whole app — everything else calls
   these functions.

8. **Replace `ping` in `server.rs`** with two `#[server]` functions that
   delegate to `orders.rs`:

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

   #[cfg(feature = "server")]
   fn dto(row: crate::orders::OrderRow) -> OrderDto {
       OrderDto { id: row.id.to_string(), item: row.item, amount: row.amount, status: row.status }
   }

   #[post("/api/orders/start", state: axum::Extension<crate::state::AppState>)]
   pub async fn start_order(order: OrderInput) -> ServerFnResult<String> {
       let row = crate::orders::insert(&state.pool, &order.item, order.amount)
           .await
           .map_err(|e| ServerFnError::new(e.to_string()))?;
       Ok(row.id.to_string())
   }

   #[get("/api/orders/list", state: axum::Extension<crate::state::AppState>)]
   pub async fn list_orders() -> ServerFnResult<Vec<OrderDto>> {
       let rows = crate::orders::list(&state.pool)
           .await
           .map_err(|e| ServerFnError::new(e.to_string()))?;
       Ok(rows.into_iter().map(dto).collect())
   }
   ```

   Two new things versus chapter 2's `ping`:

   - `state: axum::Extension<crate::state::AppState>` is an *extractor
     argument* — Dioxus's `#[get]`/`#[post]` macros let you list extra
     named parameters typed as axum extractors, and they're pulled from the
     request before your function body runs. Here it fetches the
     `AppState` you `.layer()`ed onto the router in `main.rs`, giving you
     `state.pool` inside the function.
   - `OrderRow` (from `orders.rs`, server-only) and `OrderDto` (here, sent
     over the wire) are deliberately separate types. `dto()` converts
     between them. This matters more once auth arrives — you control
     exactly which fields a client can ever see, independent of what's in
     the database row.

9. **Update the UI** in `app.rs`: a form (item + amount) posting to
   `start_order`, and a table rendering whatever `list_orders` returns.
   Look at chapter 3's `app.rs` for the full component if you get stuck —
   the pattern (signals for form fields, a signal for the list, an
   `onclick` that awaits the server fn and refreshes) is the same shape
   you'll reuse for the rest of the tutorial.

## Check your work

[chapters/03-orders-database](../03-orders-database) has the full working
version, including the `docker-compose.yml` and migration.

**Next:** [Chapter 3 — Orders database](../03-orders-database/README.md)
