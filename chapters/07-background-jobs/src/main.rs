mod app;
mod auth;
mod pages;
mod server;

#[cfg(feature = "server")]
mod jobs;
#[cfg(feature = "server")]
mod orders;
#[cfg(feature = "server")]
mod state;

use app::App;

fn main() {
    // Client entrypoint.
    #[cfg(not(feature = "server"))]
    dioxus::launch(App);

    // Server entrypoint: connect Postgres, run migrations, spawn the embedded
    // graphile_worker worker, and serve the app. better-auth.rs owns session
    // cookies internally — no tower layer needed for them.
    #[cfg(feature = "server")]
    dioxus::serve(|| async {
        let (state, _) = state::AppState::new().await;
        Ok(dioxus::server::router(App).layer(axum::Extension(state)))
    });
}
