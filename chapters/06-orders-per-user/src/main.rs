mod app;
mod auth;
mod pages;
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

    // Server entrypoint: connect Postgres, run migrations, and serve the app
    // with a cookie session layer backed by the same Postgres pool.
    #[cfg(feature = "server")]
    dioxus::serve(|| async {
        let state = state::AppState::new().await;

        let session_store = tower_sessions_sqlx_store::PostgresStore::new(state.pool.clone());
        session_store
            .migrate()
            .await
            .expect("failed to migrate session store");
        let session_layer =
            tower_sessions::SessionManagerLayer::new(session_store).with_secure(false);

        Ok(dioxus::server::router(App)
            .layer(session_layer)
            .layer(axum::Extension(state)))
    });
}
