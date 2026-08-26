//! Email deliverability routes.

use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, State},
    routing::post,
};
use openpanel_app::DeliverabilityService;
use serde::Deserialize;
use uuid::Uuid;

use crate::{ApiError, ApiResult, AuthUser};

/// Builds the Axum sub-router for deliverability checks.
pub fn router(svc: Arc<DeliverabilityService>) -> Router {
    Router::new()
        .route("/{id}/deliverability/check", post(post_check))
        .with_state(svc)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CheckInput {
    ip: std::net::IpAddr,
}

async fn post_check(
    State(svc): State<Arc<DeliverabilityService>>,
    AuthUser(caller, _): AuthUser,
    Path(_id): Path<Uuid>,
    Json(input): Json<CheckInput>,
) -> ApiResult<Json<serde_json::Value>> {
    let listed = svc
        .check_ip(&caller, input.ip)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(Json(
        serde_json::json!({ "ip": input.ip.to_string(), "listed": listed }),
    ))
}
