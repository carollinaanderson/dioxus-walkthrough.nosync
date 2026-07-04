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

#[server]
pub async fn start_order(order: OrderInput) -> ServerFnResult<String> {
    use crate::{orders, workflow};
    let instance_id = format!("order-{}", uuid::Uuid::new_v4());
    let input = serde_json::to_string(&order)?;
    workflow::client()
        .start_orchestration(instance_id.clone(), workflow::ORCHESTRATION_NAME, input)
        .await
        .map_err(ServerFnError::new)?;
    orders::insert(&instance_id, &order.item, order.amount)
        .await
        .map_err(ServerFnError::new)?;
    Ok(instance_id)
}

#[server]
pub async fn get_order_status(instance_id: String) -> ServerFnResult<OrderStatusDto> {
    use crate::{orders, workflow};
    let row = orders::get(&instance_id)
        .await
        .map_err(ServerFnError::new)?;
    let status = workflow::client()
        .get_orchestration_status(&instance_id)
        .await
        .map_err(ServerFnError::new)?;
    let (stage, actionable) = workflow::stage_from_status(&status);
    Ok(OrderStatusDto {
        instance_id: row.instance_id,
        item: row.item,
        amount: row.amount,
        stage,
        actionable,
    })
}

#[server]
pub async fn list_orders() -> ServerFnResult<Vec<OrderStatusDto>> {
    use crate::{orders, workflow};
    let client = workflow::client();
    let rows = orders::list().await.map_err(ServerFnError::new)?;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let status = client
            .get_orchestration_status(&row.instance_id)
            .await
            .map_err(ServerFnError::new)?;
        let (stage, actionable) = workflow::stage_from_status(&status);
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

#[server]
pub async fn submit_decision(instance_id: String, approve: bool) -> ServerFnResult<()> {
    use crate::workflow;
    let payload = if approve { "approve" } else { "reject" };
    workflow::client()
        .raise_event(instance_id, workflow::APPROVAL_EVENT, payload)
        .await
        .map_err(ServerFnError::new)?;
    Ok(())
}
