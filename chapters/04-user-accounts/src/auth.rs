//! Auth `#[server]` functions: register and login. There's no session yet
//! (that's chapter 5), so the browser "forgets" who's logged in the moment
//! you refresh — these just prove the register/login logic itself works.

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct CurrentUser {
    pub id: String,
    pub username: String,
}

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
        .map(|parsed| {
            Argon2::default()
                .verify_password(password.as_bytes(), &parsed)
                .is_ok()
        })
        .unwrap_or(false)
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
    Ok(CurrentUser {
        id: user.id.to_string(),
        username: user.username,
    })
}

#[post("/api/auth/login", state: axum::Extension<crate::state::AppState>)]
pub async fn login(username: String, password: String) -> ServerFnResult<CurrentUser> {
    let user = crate::users::find_by_username(&state.pool, username.trim())
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;
    // Same error for unknown user and wrong password — don't leak which.
    let user = user.ok_or_else(|| ServerFnError::new("invalid username or password"))?;
    if !verify_password(&password, &user.password_hash) {
        return Err(ServerFnError::new("invalid username or password"));
    }
    Ok(CurrentUser {
        id: user.id.to_string(),
        username: user.username,
    })
}

#[cfg(all(test, feature = "server"))]
mod tests {
    #[test]
    fn password_hash_round_trip() {
        let hash = super::hash_password("hunter2").expect("hashing should work");
        assert!(super::verify_password("hunter2", &hash));
        assert!(!super::verify_password("wrong-password", &hash));
        // Salted: hashing the same password twice yields different strings.
        let hash2 = super::hash_password("hunter2").unwrap();
        assert_ne!(hash, hash2);
    }
}
