//! `#[server]` functions for orders: now every query is scoped to the
//! logged-in user's id, not just gated on one existing.

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
    let user_id = crate::auth::require_user_id(&state).await?;
    let row = crate::orders::insert(&state.pool, &user_id, &order.item, order.amount)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;
    Ok(row.id.to_string())
}

#[get("/api/orders/list", state: axum::Extension<crate::state::AppState>)]
pub async fn list_orders() -> ServerFnResult<Vec<OrderDto>> {
    let user_id = crate::auth::require_user_id(&state).await?;
    let rows = crate::orders::list_for_user(&state.pool, &user_id)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;
    Ok(rows.into_iter().map(dto).collect())
}

#[get("/api/orders/{id}", state: axum::Extension<crate::state::AppState>)]
pub async fn get_order(id: String) -> ServerFnResult<OrderDto> {
    let user_id = crate::auth::require_user_id(&state).await?;
    let order_id = id
        .parse::<uuid::Uuid>()
        .map_err(|e| ServerFnError::new(e.to_string()))?;
    let row = crate::orders::get_for_user(&state.pool, &user_id, order_id)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?
        .ok_or_else(|| ServerFnError::new("order not found"))?;
    Ok(dto(row))
}
