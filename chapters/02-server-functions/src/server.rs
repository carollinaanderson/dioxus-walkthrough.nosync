//! Our first `#[server]` function.
//!
//! The `get` macro (from `dioxus::prelude`) does two things at once: on the
//! server it becomes a real axum route (`GET /api/ping`), and in the WASM
//! build it becomes a function that does an HTTP fetch to that same route.
//! Callers can't tell the difference — you just `.await` it.

use dioxus::prelude::*;

#[get("/api/ping")]
pub async fn ping() -> ServerFnResult<String> {
    Ok("pong from the server 🏓".to_string())
}
