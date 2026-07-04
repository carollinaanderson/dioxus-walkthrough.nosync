#![allow(non_snake_case)]

mod app;
mod server;
use app::App;

#[cfg(feature = "server")]
mod orders;

#[cfg(feature = "server")]
mod workflow;

// Client (wasm) entrypoint.
#[cfg(not(feature = "server"))]
fn main() {
    dioxus::launch(App);
}

// Server entrypoint: load env, boot the embedded duroxide runtime + Client,
// then serve the Dioxus app (which also registers the #[server] functions).
#[cfg(feature = "server")]
#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set (see .env)");

    let provider = duroxide_pg::PostgresProvider::new(&database_url)
        .await
        .expect("failed to initialize postgres provider");
    orders::init(provider.pool().clone())
        .await
        .expect("failed to initialize orders");
    workflow::init(provider).await;

    use dioxus::server::{DioxusRouterExt, ServeConfig};
    let address = dioxus::cli_config::fullstack_address_or_localhost();
    let router = axum::Router::new().serve_dioxus_application(ServeConfig::new(), App);
    let listener = tokio::net::TcpListener::bind(address)
        .await
        .expect("failed to bind to address");
    println!("listening on http://{address}");
    axum::serve(listener, router).await.unwrap();
}
