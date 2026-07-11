#![allow(non_snake_case)]
//! Protected orders page: create an order, list them. Redirects to /login
//! when unauthenticated (client-side UX only — server fns enforce auth
//! themselves via `require_user_id`). Every logged-in user still sees the
//! same global order list — chapter 6 scopes this per user.

use dioxus::prelude::*;

use crate::app::Route;
use crate::auth::{current_user, logout, CurrentUser, UNAUTHENTICATED};
use crate::server::{list_orders, start_order, OrderDto, OrderInput};

#[component]
pub fn OrdersPage() -> Element {
    let mut item = use_signal(|| "Widget".to_string());
    let mut amount = use_signal(|| "10".to_string());
    let mut orders = use_signal(Vec::<OrderDto>::new);
    let mut error = use_signal(|| Option::<String>::None);
    let mut user = use_signal(|| Option::<CurrentUser>::None);
    let nav = use_navigator();

    let refresh = move |_| async move {
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
    };

    // Client-side guard + identity for the header. The server fns are the
    // real enforcement boundary; this just gets the UX right.
    use_future(move || async move {
        match current_user().await {
            Ok(Some(u)) => user.set(Some(u)),
            Ok(None) => {
                nav.push(Route::LoginPage {});
            }
            Err(e) => error.set(Some(e.to_string())),
        }
    });

    use_future(move || async move {
        if let Ok(list) = list_orders().await {
            orders.set(list);
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
            Ok(_) => {
                error.set(None);
                if let Ok(list) = list_orders().await {
                    orders.set(list);
                }
            }
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
                    p { class: "sub", "Chapter 5: sessions protect this page." }
                }
                div { class: "row",
                    if let Some(u) = user() {
                        span { class: "who", "Signed in as {u.email}" }
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
                    button { onclick: refresh, "Refresh" }
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
                                    td { "{o.status}" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
