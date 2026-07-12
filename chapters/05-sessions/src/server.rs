//! `#[server]` functions for orders. Both now require a valid session via
//! [`dioxus_clerk::server::current_auth`] — but the result is still discarded,
//! not used to scope the query. That's chapter 6.

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct OrderInput {
    pub item: String,
    pub amount: u32,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct OrderDto {
    pub id: String,
    pub item: String,
    pub amount: i64,
    pub status: String,
}

#[cfg(feature = "server")]
fn dto(row: crate::orders::OrderRow) -> OrderDto {
    OrderDto {
        id: row.id.to_string(),
        item: row.item,
        amount: row.amount,
        status: row.status,
    }
}

#[post("/api/orders/start", state: axum::Extension<crate::state::AppState>)]
pub async fn start_order(order: OrderInput) -> ServerFnResult<String> {
    dioxus_clerk::server::current_auth()?;
    let row = crate::orders::insert(&state.pool, &order.item, order.amount)
        .await
        .map_err(ServerFnError::new)?;
    Ok(row.id.to_string())
}

#[get("/api/orders/list", state: axum::Extension<crate::state::AppState>)]
pub async fn list_orders() -> ServerFnResult<Vec<OrderDto>> {
    dioxus_clerk::server::current_auth()?;
    let rows = crate::orders::list(&state.pool)
        .await
        .map_err(ServerFnError::new)?;
    Ok(rows.into_iter().map(dto).collect())
}
