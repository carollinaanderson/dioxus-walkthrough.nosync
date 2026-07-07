#![allow(non_snake_case)]
//! Login page: username/password -> `auth::login` -> navigate to orders.

use dioxus::prelude::*;

use crate::app::Route;
use crate::auth::login;

#[component]
pub fn LoginPage() -> Element {
    let mut username = use_signal(String::new);
    let mut password = use_signal(String::new);
    let mut error = use_signal(|| Option::<String>::None);
    let nav = use_navigator();

    let submit = move |_| async move {
        match login(username(), password()).await {
            Ok(_) => {
                nav.push(Route::OrdersPage {});
            }
            Err(e) => error.set(Some(e.to_string())),
        }
    };

    rsx! {
        main { class: "wrap narrow",
            h1 { "Sign in" }
            p { class: "sub", "MyApp order pipeline demo" }
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
                        placeholder: "Password",
                    }
                    button { class: "primary", onclick: submit, "Sign in" }
                }
                if let Some(e) = error() {
                    p { class: "err", "{e}" }
                }
                p { class: "muted",
                    "No account? "
                    Link { to: Route::RegisterPage {}, "Register" }
                }
            }
        }
    }
}
