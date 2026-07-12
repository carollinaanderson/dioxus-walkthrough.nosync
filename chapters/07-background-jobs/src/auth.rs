//! Server-side auth boundary. Clerk verifies the session cookie in the
//! `ClerkAuthLayer` middleware (wired in `main.rs`); here we read the verified
//! identity out of the current request. This is the real enforcement point for
//! protected server fns — the client-side gating is only UX.

/// The Clerk user id for the current request, or an error if unauthenticated.
#[cfg(feature = "server")]
pub fn require_user_id() -> Result<String, dioxus::prelude::ServerFnError> {
    Ok(dioxus_clerk::server::current_auth()?.user_id)
}
