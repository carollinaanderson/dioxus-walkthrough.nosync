#![allow(non_snake_case)]
//! One page with three sections: register, login, and the (still-unscoped)
//! orders list from chapter 3. Nothing here remembers who you are yet — try
//! registering, then reloading the page, and notice the result disappears.

use dioxus::prelude::*;

use crate::auth::{login, register};
use crate::server::{list_orders, start_order, OrderDto, OrderInput};

pub fn App() -> Element {
    let mut reg_username = use_signal(String::new);
    let mut reg_password = use_signal(String::new);
    let mut reg_result = use_signal(|| Option::<String>::None);

    let mut login_username = use_signal(String::new);
    let mut login_password = use_signal(String::new);
    let mut login_result = use_signal(|| Option::<String>::None);

    let mut item = use_signal(|| "Widget".to_string());
    let mut amount = use_signal(|| "10".to_string());
    let mut orders = use_signal(Vec::<OrderDto>::new);
    let mut order_error = use_signal(|| Option::<String>::None);

    let do_register = move |_| async move {
        match register(reg_username(), reg_password()).await {
            Ok(user) => reg_result.set(Some(format!(
                "registered as {} ({})",
                user.username, user.id
            ))),
            Err(e) => reg_result.set(Some(format!("error: {e}"))),
        }
    };

    let do_login = move |_| async move {
        match login(login_username(), login_password()).await {
            Ok(user) => login_result.set(Some(format!(
                "logged in as {} ({})",
                user.username, user.id
            ))),
            Err(e) => login_result.set(Some(format!("error: {e}"))),
        }
    };

    let refresh_orders = move |_| async move {
        match list_orders().await {
            Ok(list) => {
                orders.set(list);
                order_error.set(None);
            }
            Err(e) => order_error.set(Some(e.to_string())),
        }
    };

    let create_order = move |_| async move {
        let amt = amount().trim().parse::<u32>().unwrap_or(0);
        match start_order(OrderInput {
            item: item(),
            amount: amt,
        })
        .await
        {
            Ok(_) => {
                order_error.set(None);
                if let Ok(list) = list_orders().await {
                    orders.set(list);
                }
            }
            Err(e) => order_error.set(Some(e.to_string())),
        }
    };

    use_future(move || async move {
        if let Ok(list) = list_orders().await {
            orders.set(list);
        }
    });

    rsx! {
        style { {CSS} }
        main { class: "wrap",
            h1 { "MyApp" }
            p { class: "sub", "Chapter 4: user accounts (no sessions yet)." }

            div { class: "row",
                section { class: "card narrow",
                    h2 { "Register" }
                    div { class: "col",
                        input {
                            value: "{reg_username}",
                            oninput: move |e| reg_username.set(e.value()),
                            placeholder: "Username",
                        }
                        input {
                            r#type: "password",
                            value: "{reg_password}",
                            oninput: move |e| reg_password.set(e.value()),
                            placeholder: "Password (min 8 chars)",
                        }
                        button { class: "primary", onclick: do_register, "Register" }
                        if let Some(r) = reg_result() {
                            p { class: "mono", "{r}" }
                        }
                    }
                }

                section { class: "card narrow",
                    h2 { "Login" }
                    div { class: "col",
                        input {
                            value: "{login_username}",
                            oninput: move |e| login_username.set(e.value()),
                            placeholder: "Username",
                        }
                        input {
                            r#type: "password",
                            value: "{login_password}",
                            oninput: move |e| login_password.set(e.value()),
                            placeholder: "Password",
                        }
                        button { class: "primary", onclick: do_login, "Login" }
                        if let Some(r) = login_result() {
                            p { class: "mono", "{r}" }
                        }
                    }
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
                        value: "{amount}",
                        oninput: move |e| amount.set(e.value()),
                        placeholder: "Amount",
                    }
                    button { class: "primary", onclick: create_order, "Create order" }
                    button { onclick: refresh_orders, "Refresh" }
                }
                if let Some(e) = order_error() {
                    p { class: "err", "Error: {e}" }
                }
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
                                tr {
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

pub const CSS: &str = r#"
:root { color-scheme: light dark; }
* { box-sizing: border-box; }
body { margin: 0; }
.wrap { max-width: 820px; margin: 0 auto; padding: 2rem 1.25rem;
  font: 15px/1.5 system-ui, -apple-system, Segoe UI, Roboto, sans-serif; }
h1 { margin: 0; font-size: 1.9rem; letter-spacing: -0.02em; }
.sub { margin: .25rem 0 1.5rem; opacity: .7; }
.card { border: 1px solid color-mix(in srgb, currentColor 15%, transparent);
  border-radius: 12px; padding: 1.1rem 1.25rem; margin-bottom: 1.25rem; flex: 1 1 240px; }
.card h2 { margin: 0 0 .8rem; font-size: 1.05rem; }
.row { display: flex; gap: .6rem; flex-wrap: wrap; }
.col { display: flex; flex-direction: column; gap: .6rem; }
input { flex: 1 1 140px; padding: .55rem .7rem; border-radius: 8px;
  border: 1px solid color-mix(in srgb, currentColor 25%, transparent);
  background: transparent; color: inherit; }
button { padding: .55rem .9rem; border-radius: 8px; border: 0; cursor: pointer;
  font-weight: 600; }
.primary { background: #4f46e5; color: #fff; }
.ghost { background: transparent; border: 1px solid
  color-mix(in srgb, currentColor 25%, transparent); color: inherit; }
table { width: 100%; border-collapse: collapse; }
th, td { text-align: left; padding: .55rem .5rem; border-bottom:
  1px solid color-mix(in srgb, currentColor 12%, transparent); vertical-align: middle; }
th { font-size: .78rem; text-transform: uppercase; letter-spacing: .05em; opacity: .6; }
.mono { font-family: ui-monospace, SFMono-Regular, Menlo, monospace; font-size: .82rem; }
.muted { opacity: .5; }
.err { color: #dc2626; }
.pill { display: inline-block; padding: .18rem .55rem; border-radius: 999px;
  font-size: .8rem; font-weight: 600;
  background: color-mix(in srgb, currentColor 12%, transparent); }
.pill.ok { background: #16a34a22; color: #16a34a; }
.pill.err { background: #dc262622; color: #dc2626; }
.pill.wait { background: #4f46e522; color: #6366f1; }
.nav { display: flex; align-items: center; justify-content: space-between;
  margin-bottom: 1.25rem; gap: .75rem; }
.nav .who { opacity: .7; font-size: .9rem; }
.narrow { max-width: 420px; }
a { color: #6366f1; }
"#;
