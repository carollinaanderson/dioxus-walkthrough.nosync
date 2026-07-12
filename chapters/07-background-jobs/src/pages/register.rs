#![allow(non_snake_case)]
//! Sign-up route: Clerk's embedded `<SignUp />` widget.

use dioxus::prelude::*;
use dioxus_clerk::SignUp;

#[component]
pub fn RegisterPage() -> Element {
    rsx! {
        main { class: "wrap narrow",
            h1 { "Create account" }
            p { class: "sub", "MyApp order pipeline demo" }
            SignUp {}
        }
    }
}
