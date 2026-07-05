#![allow(non_snake_case)]

mod app;
mod server;
use app::App;

#[cfg(feature = "server")]
mod orders;

#[cfg(feature = "server")]
mod state;

#[cfg(feature = "server")]
mod workflow;

fn main() {
    // Client (wasm) entrypoint.
    #[cfg(not(feature = "server"))]
    dioxus::launch(App);

    // Server entrypoint: load env, boot the embedded duroxide runtime + Client,
    // then serve the Dioxus app (which also registers the #[server] functions).
    #[cfg(feature = "server")]
    dioxus::serve(|| async {
        let state = state::AppState::new().await;
        Ok(dioxus::server::router(App).layer(axum::Extension(state)))
    });
}
