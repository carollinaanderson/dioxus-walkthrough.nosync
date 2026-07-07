mod app;
mod auth;
mod pages;
mod server;
use app::App;

#[cfg(feature = "server")]
mod jobs;

#[cfg(feature = "server")]
mod orders;

#[cfg(feature = "server")]
mod state;

#[cfg(feature = "server")]
mod users;

fn main() {
    // Client entrypoint.
    #[cfg(not(feature = "server"))]
    dioxus::launch(App);

    // Server entrypoint: connect Postgres, run migrations, spawn the embedded
    // graphile_worker worker, and serve the app with session + state layers.
    #[cfg(feature = "server")]
    dioxus::serve(|| async {
        let (state, worker) = state::AppState::new().await;
        tokio::spawn(async move {
            if let Err(e) = worker.run().await {
                eprintln!("graphile_worker exited: {e}");
            }
        });

        let session_store = tower_sessions_sqlx_store::PostgresStore::new(state.pool.clone());
        session_store
            .migrate()
            .await
            .expect("failed to migrate session store");
        // `with_secure(false)` so the cookie works over plain http in dev;
        // set it to true behind TLS in production.
        let session_layer = tower_sessions::SessionManagerLayer::new(session_store)
            .with_secure(false);

        Ok(dioxus::server::router(App)
            .layer(session_layer)
            .layer(axum::Extension(state)))
    });
}
