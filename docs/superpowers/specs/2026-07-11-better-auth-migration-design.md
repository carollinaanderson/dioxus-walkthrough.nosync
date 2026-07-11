# Migrate tutorial auth to better-auth.rs

## Goal

Replace the hand-rolled argon2 + tower-sessions auth stack (chapters 4–7)
with [better-auth.rs](https://better-auth.rs), while keeping the tutorial's
`#[server]`-function pattern intact and its chapter count unchanged.

## Scope

Chapters 04, 05, 06, 07 (each carries the auth code forward from the
previous one), the root `README.md` chapter table, and each touched
chapter's own `README.md`. Chapters 1–3 have no auth and are untouched.

## Decisions

1. **Chapter narrative reshape, not a merge.** better-auth.rs bundles
   password hashing and session management into one setup step, so the
   pedagogical split changes:
   - Chapter 4 ("user-accounts") becomes: configure `better-auth.rs`
     (`AuthConfig`, `SqlxAdapter`, `EmailPasswordPlugin`,
     `SessionManagementPlugin`), register/login working end-to-end.
     Sessions are already live by the end of this chapter — that's now
     inherent to wiring up the library, not a separate lesson.
   - Chapter 5 ("sessions") becomes: *use* the session that's already
     there — `require_user_id` guards, redirect-to-`/login` UX, the
     Dioxus router, logout. No new hashing/session plumbing is
     introduced here.
   - Chapter numbering, folder names, and chapter count are unchanged.

2. **Identity model: email + password, username dropped entirely.**
   better-auth.rs's `EmailPasswordPlugin` requires an email at sign-up
   (username is only usable as an additional sign-in identifier on top of
   an email account). Rather than carry both fields, the tutorial drops
   `username` and uses email as the sole identifier. `CurrentUser` becomes
   `{ id: String, email: String }`.

3. **Keep `#[server]` fn wrappers; don't mount better-auth's own
   axum_router.** The tutorial's established pattern (ch1–3) is calling
   Dioxus `#[server]` functions from the frontend. better-auth.rs's
   idiomatic path is nesting `auth.axum_router()` and having the frontend
   call its REST endpoints directly — but that breaks the established
   pattern and swaps `#[server]` fn calls for raw `fetch`/`gloo-net` calls
   mid-tutorial. Instead, `register`/`login`/`logout`/`current_user`
   stay `#[server]` fns that call `auth.handle_request(...)` internally:
   - Build a JSON body, call `auth.handle_request(AuthRequest { method,
     path, headers, body, query })` for the relevant better-auth route
     (`/sign-up/email`, `/sign-in/email`, `/sign-out`, `/get-session`).
   - Forward the incoming `Cookie` header into the request headers (read
     via Dioxus 0.7's cookie/header extractors).
   - Forward any `Set-Cookie` from the better-auth response back to the
     browser via Dioxus 0.7's `SetHeader<SetCookie>` /
     `FullstackContext::add_response_header` (confirmed available in
     dioxus 0.7 fullstack).
   - `require_user_id` (used by chapter 6+) becomes a call to
     `/get-session` with the forwarded cookie, mapping to the session's
     user id or `UNAUTHENTICATED`.

## Chapter-by-chapter changes

### Chapter 4 — user-accounts

- Dependencies: remove `argon2`; add `better-auth` with `axum` and
  `sqlx-postgres` features. Keep `sqlx`, `axum`, `tokio`, `dotenvy`,
  `uuid` (orders still use UUID ids — unrelated to this change).
- `AppState` gains `auth: Arc<BetterAuth<SqlxAdapter>>`, built with
  `AuthConfig::new(secret).base_url(...)`,
  `.plugin(EmailPasswordPlugin::new().enable_signup(true).password_min_length(8))`,
  `.plugin(SessionManagementPlugin::new())`,
  `.database(SqlxAdapter::from_pool(pool.clone()))` — shares the existing
  pool.
- New `.env.example` var: `BETTER_AUTH_SECRET` (32+ chars).
- Delete `src/users.rs` and the hand-rolled `hash_password`/
  `verify_password` functions — better-auth owns the user table and
  hashing.
- Replace migration `0002_create_users.sql` with `0002_better_auth.sql`
  creating better-auth's documented `users`, `sessions`, `accounts`,
  `verifications` tables (`TEXT` primary keys). No organization tables.
- Rewrite `auth.rs` as thin `#[server]` wrappers per decision 3.
- Delete the `hash_password` round-trip unit test (nothing hand-rolled
  left to test).
- README rewritten: framing shifts from "hand-hash with argon2" to
  "configure better-auth.rs, understand its plugin model — sessions are
  already live by the end of this chapter."

### Chapter 5 — sessions

- No new hashing/session plumbing — that's done in chapter 4 now. This
  chapter's content becomes: `require_user_id` guards, redirect-to-
  `/login` UX on `UNAUTHENTICATED`, the Dioxus router, logout button.
- `main.rs` simplifies: no `tower_sessions::SessionManagerLayer`, no
  separate session-store migration call at boot — just
  `axum::Extension(state)` (which now carries `auth`).
- README rewritten to explain protecting routes with the session that's
  already there, rather than introducing cookie sessions from scratch.

### Chapter 6 — orders-per-user

- Migration: `orders.user_id` changes from `UUID REFERENCES users(id)` to
  `TEXT REFERENCES users(id)`, matching better-auth's string user ids.
- `orders.rs`'s `list_for_user` takes `user_id: &str` instead of `Uuid`.
- README updated to note the id type and that it comes from the
  better-auth session, not a hand-rolled cookie value.

### Chapter 7 — background-jobs

- Carries forward chapter 6's `auth.rs`/`state.rs`/migrations, as it
  already does. Update any remaining `username` references (README,
  comments) to `email`.

## Root README

Update the chapter table's descriptions for rows 4 and 5 to reflect the
new narrative (better-auth.rs setup / using the session), without
changing row count, chapter titles, or folder links.

## Out of scope

- Chapters 1–3 (no auth).
- Any better-auth.rs feature beyond email/password + session management
  (no OAuth providers, no organizations, no two-factor).
- A separate end-to-end test suite — none currently exists in this repo
  beyond the per-chapter unit tests already covered above.
