//! `#[server]` functions: the bridge between the Dioxus UI and the duroxide
//! control-plane `Client`. Bodies run server-side only; the macro generates the
//! client-side stubs.

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct OrderInput {
    pub item: String,
    pub amount: u32,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct OrderStatusDto {
    pub instance_id: String,
    pub item: String,
    pub amount: i64,
    pub stage: String,
    pub actionable: bool,
}

#[server(axum::Extension(state): axum::Extension<crate::state::AppState>)]
pub async fn start_order(order: OrderInput) -> ServerFnResult<String> {
    let instance_id = format!("order-{}", uuid::Uuid::new_v4());
    let input = serde_json::to_string(&order)?;
    state
        .client
        .start_orchestration(
            instance_id.clone(),
            crate::workflow::ORCHESTRATION_NAME,
            input,
        )
        .await
        .map_err(ServerFnError::new)?;
    crate::orders::insert(&state.pool, &instance_id, &order.item, order.amount)
        .await
        .map_err(ServerFnError::new)?;
    Ok(instance_id)
}

#[server(axum::Extension(state): axum::Extension<crate::state::AppState>)]
pub async fn get_order_status(instance_id: String) -> ServerFnResult<OrderStatusDto> {
    let row = crate::orders::get(&state.pool, &instance_id)
        .await
        .map_err(ServerFnError::new)?;
    let status = state
        .client
        .get_orchestration_status(&instance_id)
        .await
        .map_err(ServerFnError::new)?;
    let (stage, actionable) = crate::workflow::stage_from_status(&status);
    Ok(OrderStatusDto {
        instance_id: row.instance_id,
        item: row.item,
        amount: row.amount,
        stage,
        actionable,
    })
}

#[server(axum::Extension(state): axum::Extension<crate::state::AppState>)]
pub async fn list_orders() -> ServerFnResult<Vec<OrderStatusDto>> {
    let rows = crate::orders::list(&state.pool)
        .await
        .map_err(ServerFnError::new)?;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let status = state
            .client
            .get_orchestration_status(&row.instance_id)
            .await
            .map_err(ServerFnError::new)?;
        let (stage, actionable) = crate::workflow::stage_from_status(&status);
        out.push(OrderStatusDto {
            instance_id: row.instance_id,
            item: row.item,
            amount: row.amount,
            stage,
            actionable,
        });
    }
    Ok(out)
}

#[server(axum::Extension(state): axum::Extension<crate::state::AppState>)]
pub async fn submit_decision(instance_id: String, approve: bool) -> ServerFnResult<()> {
    let payload = if approve { "approve" } else { "reject" };
    state
        .client
        .raise_event(instance_id, crate::workflow::APPROVAL_EVENT, payload)
        .await
        .map_err(ServerFnError::new)?;
    Ok(())
}
