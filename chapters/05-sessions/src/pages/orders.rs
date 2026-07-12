#![allow(non_snake_case)]
//! Protected orders page. `SignedOut` + `RedirectToSignIn` send anonymous
//! visitors to the sign-in route; the real orders UI only renders inside
//! `SignedIn`. Server fns still enforce auth themselves via
//! `dioxus_clerk::server::current_auth`. Every logged-in user still sees the
//! same global order list — chapter 6 scopes this per user.

use dioxus::prelude::*;
use dioxus_clerk::{RedirectToSignIn, SignedIn, SignedOut, UserButton};

use crate::server::{list_orders, start_order, OrderDto, OrderInput};

#[component]
pub fn Orders() -> Element {
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

    let refresh = move |_| async move {
        match list_orders().await {
            Ok(list) => {
                orders.set(list);
                error.set(None);
            }
            Err(e) => error.set(Some(e.to_string())),
        }
    };

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

    rsx! {
        main { class: "wrap",
            header { class: "nav",
                div {
                    h1 { "MyApp" }
                    p { class: "sub", "Chapter 5: Clerk sessions protect this page." }
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
