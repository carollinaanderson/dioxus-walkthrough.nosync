mod app;
mod server;
use app::App;

fn main() {
    // Client entrypoint: compiled to WASM, runs in the browser.
    #[cfg(not(feature = "server"))]
    dioxus::launch(App);

    // Server entrypoint: compiled natively, runs on your machine. `dx serve`
    // builds and runs both binaries for you.
    #[cfg(feature = "server")]
    dioxus::serve(|| async { Ok(dioxus::server::router(App)) });
}
