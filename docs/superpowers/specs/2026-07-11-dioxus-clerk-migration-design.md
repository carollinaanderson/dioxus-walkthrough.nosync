# Design: replace better-auth.rs with dioxus-clerk

**Date:** 2026-07-11
**Status:** Approved

## Goal

Replace the self-hosted `better-auth` crate with the hosted-auth crate
`dioxus-clerk` (v0.1.0) across chapters 4–7 of the tutorial, and update every
affected README. Auth data moves out of the app's Postgres and into Clerk's
cloud; the tutorial gains a "sign up for a free Clerk account" prerequisite.

## Decisions (from brainstorming)

1. **Go hosted with Clerk.** Local `users`/`sessions` tables
   (`0002_better_auth.sql`) are dropped. Learners need a free clerk.com account
   plus a publishable key and a secret key.
2. **Reframe chapters 4–5 around Clerk's drop-in components.** Keep 7 chapters.
   The hand-rolled register/login forms and `pages/login.rs` /
   `pages/register.rs` are replaced by Clerk components.
3. **Code + docs, best-effort build.** Aim for `cargo check` to compile per
   chapter; note any v0.1.0 feature-flag issues rather than exhaustively
   resolving them.

## Background: how the two libraries differ

`better-auth.rs` is **self-hosted**:
- Local `users` / `sessions` / `accounts` tables (migration
  `0002_better_auth.sql`), server-side password hashing.
- Custom `#[server]` fns (`auth.rs`) that bridge Dioxus server functions to
  better-auth's HTTP-shaped API (`auth.handle_request`), forwarding the session
  cookie both ways.
- Hand-built login/register pages and a client-side `current_user()` guard.
- `AppState.auth: Arc<BetterAuth<SqlxAdapter>>` shares the app's `PgPool`.

`dioxus-clerk` is **hosted**:
- `<ClerkProvider publishable_key=…>` loads clerk-js in the browser; drop-in
  components (`SignIn`, `SignUp`, `SignInButton`, `SignUpButton`, `UserButton`,
  `SignOutButton`, `SignedIn`, `SignedOut`, `RedirectToSignIn`, `Protect`, …)
  provide all auth UI.
- Server verifies the Clerk session cookie via a tower middleware
  `ClerkAuthLayer` (needs `CLERK_SECRET_KEY` at runtime); `#[server]` fns read
  the verified identity via `current_auth()? -> auth.user_id: String`.
- No local auth tables. `user_id` is a Clerk id string, e.g. `user_2abc…`.

Key config sourcing (from the crate docs):
- `CLERK_PUBLISHABLE_KEY` — build time, via `env!` (baked into both wasm and
  server binaries).
- `CLERK_SECRET_KEY` — runtime, server only; must never reach the wasm bundle.

## Cross-cutting changes (chapters 4–7)

- **Cargo.toml** — remove `better-auth`; add `dioxus-clerk`. Client (wasm/web)
  build enables its default (client) features; the `server` app feature enables
  `dioxus-clerk`'s `server` feature. **Best-effort build risk:** the crate's own
  demo uses a `fullstack-web` client feature to distinguish the native-fullstack
  client from the Cloudflare-SPA client; our tutorial is a plain
  `dioxus/web` + `dioxus/server` fullstack app. Wire the features so it
  compiles; flag anything that cannot be fully resolved on a v0.1.0 crate.
- **`.env.example`** — remove `BETTER_AUTH_SECRET`; add `CLERK_PUBLISHABLE_KEY`
  and `CLERK_SECRET_KEY`.
- **`main.rs`** — add `.layer(ClerkAuthLayer::from_env()?)` (or `::from_env()
  .expect(...)`) to the server router alongside the existing state extension.
- **`app.rs`** — wrap the router in
  `ClerkProvider { publishable_key: env!("CLERK_PUBLISHABLE_KEY"), … }`.
- **`state.rs`** — remove the `auth: Arc<BetterAuth<…>>` field and its builder;
  drop the `BETTER_AUTH_SECRET` read. State keeps `pool` (+ `worker` in ch7).
- **`auth.rs`** — delete the entire better-auth HTTP bridge. Replace with a thin
  server-only helper: `require_user_id(...)` → `current_auth()?.user_id`.
  `CurrentUser`, `register`, `login`, `logout`, `current_user` server fns are
  removed (Clerk owns them client-side). Keep the `UNAUTHENTICATED` marker only
  if a consumer still needs it; otherwise remove.
- **Drop `migrations/0002_better_auth.sql`** in every chapter that has it.

## Per-chapter

### Ch4 — user-accounts
Reframed as "Add hosted accounts with Clerk." `app.rs` gains `ClerkProvider`
plus `SignedOut { SignInButton / SignUpButton }` and
`SignedIn { UserButton {} }` around the existing orders section. Custom
register/login form sections removed. `auth.rs` (register/login server fns)
removed; this chapter has no protected server fn yet, so no `require_user_id`.

### Ch5 — sessions
Reframed around **gating + protected routes the Clerk way**. `pages/login.rs`
and `pages/register.rs` become embedded-widget routes: `<SignIn/>` at
`/sign-in`, `<SignUp/>` at `/sign-up` (path routing). The orders route guards
with `SignedIn` / `SignedOut` + `RedirectToSignIn` instead of the hand-rolled
`current_user()` navigation guard. Header uses `UserButton` / `SignOutButton`.

### Ch6 — orders-per-user
`0003_add_user_id_to_orders.sql` changes
`user_id TEXT REFERENCES users(id) ON DELETE CASCADE` → plain
`user_id TEXT NOT NULL` (no local `users` table to reference). It now stores the
Clerk user id string. `require_user_id` reads `current_auth()`. Orders queries
(`orders.rs`) are unchanged — they already key on a `String` user_id.

### Ch7 — background-jobs
Same auth swap as ch6. The `graphile_worker` sqlx-0.8 pin **stays** (still
shared with our own orders queries), but its Cargo.toml comment is rewritten —
it is no longer about better-auth sharing the pool. `pages/orders.rs` header
uses `UserButton` / `SignOutButton`; the polling-loop `UNAUTHENTICATED` redirect
becomes Clerk gating.

## READMEs

- **Top-level `README.md`** — rewrite chapter-table rows for ch4 and ch5 (Clerk,
  not better-auth.rs); add a free **clerk.com account + keys** item to
  "Before you start."
- **Chapter READMEs 4–7** — rewrite auth sections: Clerk setup, the two env
  vars, drop-in components, `ClerkAuthLayer` + `current_auth()`. Chapters 1–3
  READMEs are untouched.

## Verification

Best-effort. Drive toward `cargo check` compiling per chapter (both `web` and
`server` features). No live Clerk account is needed to compile — `env!` only
needs the vars present, so dummy values suffice. Note any v0.1.0 feature-flag
issues encountered.

## Out of scope

Organizations, MFA, waitlist, step-up reverification, bearer-token `/api`
routes, and other Clerk features not present in the original tutorial.
