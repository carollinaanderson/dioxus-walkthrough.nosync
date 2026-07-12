#![allow(non_snake_case)]
//! Sign-in route: Clerk's embedded `<SignIn />` widget. Path routing keeps
//! Clerk's own child paths (e.g. SSO callbacks) under `/login`.

use dioxus::prelude::*;
use dioxus_clerk::SignIn;

#[component]
pub fn Login() -> Element {
    rsx! {
        main { class: "wrap narrow",
            h1 { "Sign in" }
            p { class: "sub", "MyApp order pipeline demo" }
            SignIn {}
        }
    }
}
