//! `#[server]` functions for orders: the bridge between the Dioxus UI and the
//! Postgres order store. Each fn has an explicit HTTP endpoint so the API is
//! stable and curl-able.

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
    let row = crate::orders::insert(&state.pool, &order.item, order.amount)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;
    Ok(row.id.to_string())
}

#[get("/api/orders/list", state: axum::Extension<crate::state::AppState>)]
pub async fn list_orders() -> ServerFnResult<Vec<OrderDto>> {
    let rows = crate::orders::list(&state.pool)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;
    Ok(rows.into_iter().map(dto).collect())
}
