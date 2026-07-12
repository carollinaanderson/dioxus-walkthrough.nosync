#![allow(non_snake_case)]
//! Protected orders page: create an order, watch the graphile_worker pipeline
//! advance its status via polling. Clerk gates the page (`RedirectToSignIn`);
//! server fns enforce auth themselves via `require_user_id`.

use dioxus::prelude::*;
use dioxus_clerk::{RedirectToSignIn, SignedIn, SignedOut, UserButton};

use crate::app::status_class;
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
    rsx! {
        SignedOut { RedirectToSignIn {} }
        SignedIn { OrdersView {} }
    }
}

#[component]
fn OrdersView() -> Element {
    let mut item = use_signal(|| "Widget".to_string());
    let mut amount = use_signal(|| "10".to_string());
    let mut orders = use_signal(Vec::<OrderDto>::new);
    let mut error = use_signal(|| Option::<String>::None);

    // Poll the order list roughly every 1.5s so status transitions show live.
    use_future(move || async move {
        loop {
            match list_orders().await {
                Ok(list) => {
                    orders.set(list);
                    error.set(None);
                }
                Err(e) => error.set(Some(e.to_string())),
            }
            sleep_ms(1500).await;
        }
    });

    let create = move |_| async move {
        let amt = amount().trim().parse::<u32>().unwrap_or(0);
        match start_order(OrderInput { item: item(), amount: amt }).await {
            Ok(_) => error.set(None),
            Err(e) => error.set(Some(e.to_string())),
        }
    };

    rsx! {
        main { class: "wrap",
            header { class: "nav",
                div {
                    h1 { "MyApp" }
                    p { class: "sub", "Dioxus server functions driving a graphile_worker order pipeline." }
                }
                div { class: "row",
                    UserButton {}
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
