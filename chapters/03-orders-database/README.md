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

Chapter 4 adds a `users` table and register/login — but *without* wiring
sessions yet, so you can focus purely on the auth logic first.

1. **Copy this chapter as your working copy:**

   ```bash
   cp -r ../03-orders-database ../my-04-user-accounts
   cd ../my-04-user-accounts
   ```

2. **Update `docker-compose.yml` / `.env`** to a new db name and port (`5434`
   / `myapp_ch04` in the reference code) so this chapter doesn't collide
   with a chapter-3 Postgres you might still have running.

3. **Add a migration**, `migrations/0002_create_users.sql`:

   ```sql
   CREATE TABLE IF NOT EXISTS users (
       id            UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
       username      TEXT        NOT NULL UNIQUE,
       password_hash TEXT        NOT NULL,
       created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
   );
   ```

   `UNIQUE` on `username` matters: it's Postgres, not your Rust code,
   guaranteeing no two rows ever share a username — even if two register
   requests race each other at the exact same moment. `password_hash` is
   named that way on purpose; there's no `password` column, because a raw
   password should never reach the database.

4. **Add `argon2` as a server-only dependency:**

   ```toml
   argon2 = { version = "0.5", optional = true }
   ```

   and `"dep:argon2"` to the `server` feature list.
   [argon2](https://docs.rs/argon2) is a password-hashing algorithm — slow
   and memory-hard *on purpose*, so that even if your database leaks,
   brute-forcing the original passwords out of the hashes is expensive.

5. **Write `src/users.rs`**, the same shape of store module as
   `orders.rs`:

   ```rust
   use sqlx::{prelude::FromRow, PgPool};
   use uuid::Uuid;

   #[derive(Debug, Clone, FromRow)]
   pub struct UserRow {
       pub id: Uuid,
       pub username: String,
       pub password_hash: String,
   }

   pub async fn insert(pool: &PgPool, username: &str, password_hash: &str) -> Result<UserRow, sqlx::Error> {
       sqlx::query_as::<_, UserRow>(
           "INSERT INTO users (username, password_hash) VALUES ($1, $2)
            RETURNING id, username, password_hash",
       )
       .bind(username)
       .bind(password_hash)
       .fetch_one(pool)
       .await
   }

   pub async fn find_by_username(pool: &PgPool, username: &str) -> Result<Option<UserRow>, sqlx::Error> {
       sqlx::query_as::<_, UserRow>("SELECT id, username, password_hash FROM users WHERE username = $1")
           .bind(username)
           .fetch_optional(pool)
           .await
   }
   ```

   `find_by_username` returns `Option<UserRow>` (via `.fetch_optional`), not
   `UserRow` — "no such user" is an expected, ordinary outcome here, not an
   error, so the type says so.

6. **Create `src/auth.rs`** with the hashing helpers:

   ```rust
   #[cfg(feature = "server")]
   pub(crate) fn hash_password(password: &str) -> Result<String, String> {
       use argon2::password_hash::{rand_core::OsRng, SaltString};
       use argon2::{Argon2, PasswordHasher};
       let salt = SaltString::generate(&mut OsRng);
       Argon2::default()
           .hash_password(password.as_bytes(), &salt)
           .map(|h| h.to_string())
           .map_err(|e| e.to_string())
   }

   #[cfg(feature = "server")]
   fn verify_password(password: &str, hash: &str) -> bool {
       use argon2::{Argon2, PasswordHash, PasswordVerifier};
       PasswordHash::new(hash)
           .map(|parsed| Argon2::default().verify_password(password.as_bytes(), &parsed).is_ok())
           .unwrap_or(false)
   }
   ```

   `SaltString::generate(&mut OsRng)` makes a fresh random salt *per call* —
   that's why hashing the same password twice gives two different strings
   (see the test in the reference code). The salt gets embedded in the
   output string itself, so `verify_password` doesn't need it passed in
   separately: `PasswordHash::new(hash)` parses it back out. `verify_password`
   collapses every failure mode (bad hash format, wrong password) into
   `false` with `.unwrap_or(false)` — callers only need a yes/no answer.

7. **Add `register` and `login` server functions** to `auth.rs`:

   ```rust
   use dioxus::prelude::*;
   use serde::{Deserialize, Serialize};

   #[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
   pub struct CurrentUser {
       pub id: String,
       pub username: String,
   }

   #[post("/api/auth/register", state: axum::Extension<crate::state::AppState>)]
   pub async fn register(username: String, password: String) -> ServerFnResult<CurrentUser> {
       let username = username.trim().to_string();
       if username.is_empty() {
           return Err(ServerFnError::new("username is required"));
       }
       if password.len() < 8 {
           return Err(ServerFnError::new("password must be at least 8 characters"));
       }
       if crate::users::find_by_username(&state.pool, &username)
           .await
           .map_err(|e| ServerFnError::new(e.to_string()))?
           .is_some()
       {
           return Err(ServerFnError::new("username already taken"));
       }
       let hash = hash_password(&password).map_err(ServerFnError::new)?;
       let user = crate::users::insert(&state.pool, &username, &hash)
           .await
           .map_err(|e| ServerFnError::new(e.to_string()))?;
       Ok(CurrentUser { id: user.id.to_string(), username: user.username })
   }

   #[post("/api/auth/login", state: axum::Extension<crate::state::AppState>)]
   pub async fn login(username: String, password: String) -> ServerFnResult<CurrentUser> {
       let user = crate::users::find_by_username(&state.pool, username.trim())
           .await
           .map_err(|e| ServerFnError::new(e.to_string()))?;
       let user = user.ok_or_else(|| ServerFnError::new("invalid username or password"))?;
       if !verify_password(&password, &user.password_hash) {
           return Err(ServerFnError::new("invalid username or password"));
       }
       Ok(CurrentUser { id: user.id.to_string(), username: user.username })
   }
   ```

   Notice `login` returns the exact same error message,
   `"invalid username or password"`, whether the username doesn't exist or
   the password is wrong. If those errors differed, an attacker could send
   guesses and learn which usernames are registered just from which error
   comes back — a real information leak called username enumeration.
   Neither function stores anything in a session — that's chapter 5, so for
   now the browser has no memory of who just registered or logged in.

8. **Add register/login forms to the UI**, each calling their server
   function and displaying the `CurrentUser` (or error) that comes back.
   Try registering, then reloading the page — the result you saw is gone,
   because nothing on the server or in the browser remembered it. That gap
   is exactly what chapter 5 fills.

## Check your work

[chapters/04-user-accounts](../04-user-accounts) has the full version,
including a unit test for the argon2 hash/verify round-trip.

**Next:** [Chapter 4 — User accounts](../04-user-accounts/README.md)
