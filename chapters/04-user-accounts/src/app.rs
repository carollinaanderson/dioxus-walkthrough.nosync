#![allow(non_snake_case)]
//! One page. Clerk owns accounts now: signed-out visitors get sign-in /
//! sign-up buttons; signed-in visitors get a Clerk `UserButton` (avatar menu
//! with sign-out) and the orders section from chapter 3. The orders list is
//! still global — chapter 6 scopes it per user.

use dioxus::prelude::*;
use dioxus_clerk::{ClerkProvider, SignInButton, SignUpButton, SignedIn, SignedOut, UserButton};

use crate::server::{list_orders, start_order, OrderDto, OrderInput};

pub fn App() -> Element {
    rsx! {
        style { {CSS} }
        ClerkProvider { publishable_key: env!("CLERK_PUBLISHABLE_KEY"),
            main { class: "wrap",
                header { class: "nav",
                    div {
                        h1 { "MyApp" }
                        p { class: "sub", "Chapter 4: user accounts via Clerk." }
                    }
                    div { class: "row",
                        SignedOut {
                            SignInButton { class: "primary", "Sign in" }
                            SignUpButton { class: "ghost", "Create account" }
                        }
                        SignedIn { UserButton {} }
                    }
                }

                SignedOut {
                    section { class: "card",
                        p { class: "muted", "Sign in or create an account to place orders." }
                    }
                }

                SignedIn { OrdersSection {} }
            }
        }
    }
}

#[component]
fn OrdersSection() -> Element {
    let mut item = use_signal(|| "Widget".to_string());
    let mut amount = use_signal(|| "10".to_string());
    let mut orders = use_signal(Vec::<OrderDto>::new);
    let mut order_error = use_signal(|| Option::<String>::None);

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
        match start_order(OrderInput { item: item(), amount: amt }).await {
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
