#![allow(non_snake_case)]
use dioxus::prelude::*;

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    rsx! {
        h1 { "Duroxus — Order Approval PoC" }
        p { "Scaffold online." }
    }
}
