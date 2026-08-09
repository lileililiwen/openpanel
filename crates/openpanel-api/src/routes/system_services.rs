//! Authenticated allowlisted service inventory and lifecycle routes.
use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{get, post},
};
use openpanel_app::system_services::{
    ManagedServiceStatus, ServiceImpact, ServiceManager, ServiceManagerError,
};
use openpanel_domain::system_services::{Health, ServiceAction};
use serde::Deserialize;

use crate::{ApiError, ApiResult, AuthUser};
/// Build `/services` routes.
pub fn router(service: Arc<ServiceManager>) -> Router {
    Router::new()
        .route("/", get(inventory))
        .route("/{id}", get(status))
        .route("/{id}/preview", post(preview))
        .route("/{id}/actions", post(action))
        .route("/{id}/history", get(history))
        .route("/{id}/logs", get(logs))
        .with_state(service)
}
#[derive(Deserialize)]
struct ActionInput {
    action: ServiceAction,
    #[serde(default)]
    confirmed: bool,
}
async fn inventory(
    State(service): State<Arc<ServiceManager>>,
    _: AuthUser,
) -> ApiResult<Json<Vec<ManagedServiceStatus>>> {
    Ok(Json(service.inventory().await.map_err(map)?))
}
async fn status(
    State(service): State<Arc<ServiceManager>>,
    _: AuthUser,
    Path(id): Path<String>,
) -> ApiResult<Json<ManagedServiceStatus>> {
    Ok(Json(service.status(&id).await.map_err(map)?))
}
async fn preview(
    State(service): State<Arc<ServiceManager>>,
    _: AuthUser,
    Path(id): Path<String>,
    Json(input): Json<ActionInput>,
) -> ApiResult<Json<ServiceImpact>> {
    Ok(Json(service.preview(&id, input.action).map_err(map)?))
}
async fn action(
    State(service): State<Arc<ServiceManager>>,
    AuthUser(user, _): AuthUser,
    Path(id): Path<String>,
    Json(input): Json<ActionInput>,
) -> ApiResult<Json<ManagedServiceStatus>> {
    Ok(Json(
        service
            .perform(user.id(), user.role(), &id, input.action, input.confirmed)
            .await
            .map_err(map)?,
    ))
}
async fn history(
    State(service): State<Arc<ServiceManager>>,
    _: AuthUser,
    Path(id): Path<String>,
) -> ApiResult<Json<Vec<Health>>> {
    Ok(Json(service.history(&id, 100).await.map_err(map)?))
}
async fn logs(
    State(service): State<Arc<ServiceManager>>,
    _: AuthUser,
    Path(id): Path<String>,
) -> ApiResult<Json<Vec<String>>> {
    Ok(Json(service.logs(&id, 100).await.map_err(map)?))
}
fn map(error: ServiceManagerError) -> ApiError {
    match error {
        ServiceManagerError::NotFound => ApiError::NotFound("service".into()),
        ServiceManagerError::Forbidden => ApiError::Forbidden,
        ServiceManagerError::ConfirmationRequired => ApiError::Conflict(error.to_string()),
        ServiceManagerError::Unsupported => ApiError::Unprocessable(error.to_string()),
        ServiceManagerError::Controller
        | ServiceManagerError::Readiness
        | ServiceManagerError::Persistence => ApiError::Internal(error.to_string()),
    }
}
