#![allow(non_snake_case)]
//! The single-page UI: create an order, watch the durable workflow's stage via
//! polling, and Approve/Reject an order that is awaiting a human decision.

use dioxus::prelude::*;

use crate::server::{list_orders, start_order, submit_decision, OrderInput, OrderStatusDto};

/// Interval sleep for the polling loop. The loop only ever runs on the wasm
/// client (`use_future` does not run during native SSR); the non-wasm arm just
/// has to compile, so it parks forever without pulling in a native timer crate.
async fn sleep_ms(_ms: u32) {
    #[cfg(target_arch = "wasm32")]
    gloo_timers::future::TimeoutFuture::new(_ms).await;
    #[cfg(not(target_arch = "wasm32"))]
    std::future::pending::<()>().await;
}

pub fn App() -> Element {
    let mut item = use_signal(|| "Widget".to_string());
    let mut amount = use_signal(|| "10".to_string());
    let mut orders = use_signal(Vec::<OrderStatusDto>::new);
    let mut error = use_signal(|| Option::<String>::None);

    // Poll the order list roughly every 1.5s so status transitions show live.
    use_future(move || async move {
        loop {
            match list_orders().await {
                Ok(list) => orders.set(list),
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
        style { {CSS} }
        main { class: "wrap",
            h1 { "Duroxus" }
            p { class: "sub", "Dioxus server functions driving a durable duroxide order-approval workflow." }

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
                            tr { th { "Item" } th { "Amount" } th { "Instance" } th { "Stage" } th { "Action" } }
                        }
                        tbody {
                            for o in orders() {
                                OrderRow { key: "{o.instance_id}", order: o.clone() }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn OrderRow(order: OrderStatusDto) -> Element {
    let approve_id = order.instance_id.clone();
    let reject_id = order.instance_id.clone();
    rsx! {
        tr {
            td { "{order.item}" }
            td { "{order.amount}" }
            td { class: "mono", "{order.instance_id}" }
            td {
                span { class: stage_class(&order.stage), "{order.stage}" }
            }
            td {
                if order.actionable {
                    button {
                        class: "approve",
                        onclick: move |_| {
                            let id = approve_id.clone();
                            async move { let _ = submit_decision(id, true).await; }
                        },
                        "Approve"
                    }
                    button {
                        class: "reject",
                        onclick: move |_| {
                            let id = reject_id.clone();
                            async move { let _ = submit_decision(id, false).await; }
                        },
                        "Reject"
                    }
                } else {
                    span { class: "muted", "—" }
                }
            }
        }
    }
}

fn stage_class(stage: &str) -> &'static str {
    if stage.starts_with("Fulfilled") {
        "pill ok"
    } else if stage.starts_with("Refunded") {
        "pill warn"
    } else if stage.starts_with("Failed") {
        "pill err"
    } else if stage.starts_with("Awaiting") {
        "pill wait"
    } else {
        "pill"
    }
}

const CSS: &str = r#"
:root { color-scheme: light dark; }
* { box-sizing: border-box; }
body { margin: 0; }
.wrap { max-width: 820px; margin: 0 auto; padding: 2rem 1.25rem;
  font: 15px/1.5 system-ui, -apple-system, Segoe UI, Roboto, sans-serif; }
h1 { margin: 0; font-size: 1.9rem; letter-spacing: -0.02em; }
.sub { margin: .25rem 0 1.5rem; opacity: .7; }
.card { border: 1px solid color-mix(in srgb, currentColor 15%, transparent);
  border-radius: 12px; padding: 1.1rem 1.25rem; margin-bottom: 1.25rem; }
.card h2 { margin: 0 0 .8rem; font-size: 1.05rem; }
.row { display: flex; gap: .6rem; flex-wrap: wrap; }
input { flex: 1 1 140px; padding: .55rem .7rem; border-radius: 8px;
  border: 1px solid color-mix(in srgb, currentColor 25%, transparent);
  background: transparent; color: inherit; }
button { padding: .55rem .9rem; border-radius: 8px; border: 0; cursor: pointer;
  font-weight: 600; }
.primary { background: #4f46e5; color: #fff; }
.approve { background: #16a34a; color: #fff; margin-right: .4rem; }
.reject { background: #dc2626; color: #fff; }
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
.pill.warn { background: #d9770622; color: #d97706; }
.pill.err { background: #dc262622; color: #dc2626; }
.pill.wait { background: #4f46e522; color: #6366f1; }
"#;
