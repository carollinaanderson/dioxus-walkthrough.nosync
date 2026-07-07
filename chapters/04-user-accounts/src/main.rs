mod app;
mod auth;
mod server;

#[cfg(feature = "server")]
mod orders;
#[cfg(feature = "server")]
mod state;
#[cfg(feature = "server")]
mod users;

use app::App;

fn main() {
    // Client entrypoint.
    #[cfg(not(feature = "server"))]
    dioxus::launch(App);

    // Server entrypoint: connect Postgres, run migrations, serve the app.
    #[cfg(feature = "server")]
    dioxus::serve(|| async {
        let state = state::AppState::new().await;
        Ok(dioxus::server::router(App).layer(axum::Extension(state)))
    });
}
