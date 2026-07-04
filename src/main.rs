#![allow(non_snake_case)]

mod app;
mod server;
use app::App;

#[cfg(feature = "server")]
mod workflow;

fn main() {
    dioxus::launch(App);
}
