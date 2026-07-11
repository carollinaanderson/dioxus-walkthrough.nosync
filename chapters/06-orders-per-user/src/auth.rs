//! Auth `#[server]` functions: register, login, logout, current_user.
//! Password hashing and sessions are both handled by better-auth.rs
//! (`AppState::auth`, built in `state.rs`) — this file only translates
//! between Dioxus `#[server]` fns and better-auth's HTTP-shaped API
//! (`auth.handle_request`), forwarding the session cookie in both
//! directions.

#[cfg(feature = "server")]
use std::collections::HashMap;

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct CurrentUser {
    pub id: String,
    pub email: String,
}

/// Error message for unauthenticated requests. The UI matches on this exact
/// string to redirect to the login page — keep it in sync with
/// `pages/orders.rs`.
pub const UNAUTHENTICATED: &str = "unauthenticated";

#[cfg(feature = "server")]
async fn incoming_cookie_header() -> Option<String> {
    let headers: axum::http::HeaderMap =
        dioxus::fullstack::FullstackContext::extract().await.ok()?;
    headers
        .get(axum::http::header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string)
}

#[cfg(feature = "server")]
fn forward_set_cookie(response: &better_auth::types::AuthResponse) {
    let Some(ctx) = dioxus::fullstack::FullstackContext::current() else {
        return;
    };
    for (name, value) in response.headers.iter() {
        if name.eq_ignore_ascii_case("set-cookie") {
            if let Ok(value) = axum::http::HeaderValue::from_str(value) {
                ctx.add_response_header(axum::http::header::SET_COOKIE, value);
            }
        }
    }
}

/// Call one of better-auth's routes (`/sign-up/email`, `/sign-in/email`,
/// `/sign-out`, `/get-session`) and forward cookies both ways.
#[cfg(feature = "server")]
async fn call_auth(
    state: &crate::state::AppState,
    method: better_auth::types::HttpMethod,
    path: &str,
    body: Option<serde_json::Value>,
) -> Result<better_auth::types::AuthResponse, ServerFnError> {
    let mut headers = HashMap::new();
    if body.is_some() {
        headers.insert("content-type".to_string(), "application/json".to_string());
    }
    if let Some(cookie) = incoming_cookie_header().await {
        headers.insert("cookie".to_string(), cookie);
    }
    let body_bytes = body
        .map(|b| serde_json::to_vec(&b))
        .transpose()
        .map_err(|e| ServerFnError::new(e.to_string()))?;

    let response = state
        .auth
        .handle_request(better_auth::types::AuthRequest::from_parts(
            method,
            path.to_string(),
            headers,
            body_bytes,
            HashMap::new(),
        ))
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

    forward_set_cookie(&response);
    Ok(response)
}

#[cfg(feature = "server")]
fn parse_user(response: &better_auth::types::AuthResponse) -> Result<CurrentUser, ServerFnError> {
    let json: serde_json::Value =
        serde_json::from_slice(&response.body).map_err(|e| ServerFnError::new(e.to_string()))?;
    let user = json
        .get("user")
        .ok_or_else(|| ServerFnError::new("better-auth response missing user"))?;
    Ok(CurrentUser {
        id: user
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ServerFnError::new("better-auth user missing id"))?
            .to_string(),
        email: user
            .get("email")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ServerFnError::new("better-auth user missing email"))?
            .to_string(),
    })
}

#[post("/api/auth/register", state: axum::Extension<crate::state::AppState>)]
pub async fn register(email: String, password: String) -> ServerFnResult<CurrentUser> {
    let email = email.trim().to_string();
    if email.is_empty() {
        return Err(ServerFnError::new("email is required"));
    }
    if password.len() < 8 {
        return Err(ServerFnError::new("password must be at least 8 characters"));
    }
    let response = call_auth(
        &state,
        better_auth::types::HttpMethod::Post,
        "/sign-up/email",
        Some(serde_json::json!({
            "email": email,
            "password": password,
            "name": email,
        })),
    )
    .await?;
    parse_user(&response)
}

#[post("/api/auth/login", state: axum::Extension<crate::state::AppState>)]
pub async fn login(email: String, password: String) -> ServerFnResult<CurrentUser> {
    let response = call_auth(
        &state,
        better_auth::types::HttpMethod::Post,
        "/sign-in/email",
        Some(serde_json::json!({ "email": email.trim(), "password": password })),
    )
    .await?;
    parse_user(&response)
}

#[post("/api/auth/logout", state: axum::Extension<crate::state::AppState>)]
pub async fn logout() -> ServerFnResult<()> {
    call_auth(
        &state,
        better_auth::types::HttpMethod::Post,
        "/sign-out",
        None,
    )
    .await?;
    Ok(())
}

#[get("/api/auth/me", state: axum::Extension<crate::state::AppState>)]
pub async fn current_user() -> ServerFnResult<Option<CurrentUser>> {
    let response = call_auth(
        &state,
        better_auth::types::HttpMethod::Get,
        "/get-session",
        None,
    )
    .await?;
    if response.status == 401 || response.status == 404 {
        return Ok(None);
    }
    Ok(parse_user(&response).ok())
}

/// Extract the logged-in user's id from the session, or fail with
/// [`UNAUTHENTICATED`]. This is the server-side auth boundary for every
/// protected server fn — the client-side route guard is only UX.
#[cfg(feature = "server")]
pub async fn require_user_id(state: &crate::state::AppState) -> Result<String, ServerFnError> {
    let response = call_auth(
        state,
        better_auth::types::HttpMethod::Get,
        "/get-session",
        None,
    )
    .await?;
    if response.status != 200 {
        return Err(ServerFnError::new(UNAUTHENTICATED));
    }
    parse_user(&response)
        .map(|u| u.id)
        .map_err(|_| ServerFnError::new(UNAUTHENTICATED))
}
