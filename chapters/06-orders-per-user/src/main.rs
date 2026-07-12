mod app;
mod pages;
mod server;

#[cfg(feature = "server")]
mod orders;
#[cfg(feature = "server")]
mod state;

use app::App;

fn main() {
    // Client entrypoint.
    #[cfg(not(feature = "server"))]
    dioxus::launch(App);

    // Server entrypoint: connect Postgres, run migrations, and serve the app.
    // `ClerkAuthLayer` verifies the Clerk session cookie on every request so
    // server functions can read the caller's identity via `current_auth()`.
    #[cfg(feature = "server")]
    dioxus::serve(|| async {
        let state = state::AppState::new().await;
        let clerk = dioxus_clerk::server::ClerkAuthLayer::from_env()
            .expect("CLERK_SECRET_KEY must be set (see .env)");
        Ok(dioxus::server::router(App)
            .layer(clerk)
            .layer(axum::Extension(state)))
    });
}
