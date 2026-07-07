#![allow(non_snake_case)]
//! Register page: username/password -> `auth::register` (auto-login) ->
//! navigate to orders.

use dioxus::prelude::*;

use crate::app::Route;
use crate::auth::register;

#[component]
pub fn RegisterPage() -> Element {
    let mut username = use_signal(String::new);
    let mut password = use_signal(String::new);
    let mut error = use_signal(|| Option::<String>::None);
    let nav = use_navigator();

    let submit = move |_| async move {
        match register(username(), password()).await {
            Ok(_) => {
                nav.push(Route::OrdersPage {});
            }
            Err(e) => error.set(Some(e.to_string())),
        }
    };

    rsx! {
        main { class: "wrap narrow",
            h1 { "Create account" }
            p { class: "sub", "Duroxus order pipeline demo" }
            section { class: "card",
                div { class: "col",
                    input {
                        value: "{username}",
                        oninput: move |e| username.set(e.value()),
                        placeholder: "Username",
                    }
                    input {
                        r#type: "password",
                        value: "{password}",
                        oninput: move |e| password.set(e.value()),
                        placeholder: "Password (min 8 chars)",
                    }
                    button { class: "primary", onclick: submit, "Register" }
                }
                if let Some(e) = error() {
                    p { class: "err", "{e}" }
                }
                p { class: "muted",
                    "Already have an account? "
                    Link { to: Route::LoginPage {}, "Sign in" }
                }
            }
        }
    }
}
