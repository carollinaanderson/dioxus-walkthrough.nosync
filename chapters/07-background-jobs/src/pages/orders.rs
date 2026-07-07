#![allow(non_snake_case)]
//! Protected orders page: create an order, watch the graphile_worker pipeline
//! advance its status via polling. Redirects to /login when unauthenticated
//! (client-side UX only — server fns enforce auth themselves).

use dioxus::prelude::*;

use crate::app::{status_class, Route};
use crate::auth::{current_user, logout, CurrentUser, UNAUTHENTICATED};
use crate::server::{list_orders, start_order, OrderDto, OrderInput};

/// Interval sleep for the polling loop. The loop only ever runs on the wasm
/// client (`use_future` does not run during native SSR); the non-wasm arm just
/// has to compile, so it parks forever without pulling in a native timer crate.
async fn sleep_ms(_ms: u32) {
    #[cfg(target_arch = "wasm32")]
    gloo_timers::future::TimeoutFuture::new(_ms).await;
    #[cfg(not(target_arch = "wasm32"))]
    std::future::pending::<()>().await;
}

#[component]
pub fn OrdersPage() -> Element {
    let mut item = use_signal(|| "Widget".to_string());
    let mut amount = use_signal(|| "10".to_string());
    let mut orders = use_signal(Vec::<OrderDto>::new);
    let mut error = use_signal(|| Option::<String>::None);
    let mut user = use_signal(|| Option::<CurrentUser>::None);
    let nav = use_navigator();

    // Client-side guard + identity for the header.
    use_future(move || async move {
        match current_user().await {
            Ok(Some(u)) => user.set(Some(u)),
            Ok(None) => {
                nav.push(Route::LoginPage {});
            }
            Err(e) => error.set(Some(e.to_string())),
        }
    });

    // Poll the order list roughly every 1.5s so status transitions show live.
    use_future(move || async move {
        loop {
            match list_orders().await {
                Ok(list) => {
                    orders.set(list);
                    error.set(None);
                }
                Err(e) => {
                    let msg = e.to_string();
                    if msg.contains(UNAUTHENTICATED) {
                        nav.push(Route::LoginPage {});
                    } else {
                        error.set(Some(msg));
                    }
                }
            }
            sleep_ms(1500).await;
        }
    });

    let create = move |_| async move {
        let amt = amount().trim().parse::<u32>().unwrap_or(0);
        match start_order(OrderInput {
            item: item(),
            amount: amt,
        })
        .await
        {
            Ok(_) => error.set(None),
            Err(e) => error.set(Some(e.to_string())),
        }
    };

    let sign_out = move |_| async move {
        let _ = logout().await;
        nav.push(Route::LoginPage {});
    };

    rsx! {
        main { class: "wrap",
            header { class: "nav",
                div {
                    h1 { "MyApp" }
                    p { class: "sub", "Dioxus server functions driving a graphile_worker order pipeline." }
                }
                div { class: "row",
                    if let Some(u) = user() {
                        span { class: "who", "Signed in as {u.username}" }
                    }
                    button { class: "ghost", onclick: sign_out, "Sign out" }
                }
            }

            section { class: "card",
                h2 { "New order" }
                div { class: "row",
                    input {
                        value: "{item}",
                        oninput: move |e| item.set(e.value()),
                        placeholder: "Item",
                    }
                    input {
                        r#type: "number",
                        value: "{amount}",
                        oninput: move |e| amount.set(e.value()),
                        placeholder: "Amount",
                    }
                    button { class: "primary", onclick: create, "Create order" }
                }
            }

            if let Some(e) = error() {
                p { class: "err", "Error: {e}" }
            }

            section { class: "card",
                h2 { "Orders" }
                if orders().is_empty() {
                    p { class: "muted", "No orders yet — create one above." }
                } else {
                    table {
                        thead {
                            tr { th { "Item" } th { "Amount" } th { "Id" } th { "Status" } }
                        }
                        tbody {
                            for o in orders() {
                                tr { key: "{o.id}",
                                    td { "{o.item}" }
                                    td { "{o.amount}" }
                                    td { class: "mono", "{o.id}" }
                                    td { span { class: status_class(&o.status), "{o.status}" } }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
