//! Owner-only Software Center catalog, preview, execution, and job routes.

use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{get, post},
};
use openpanel_app::software_center::{
    ApplicationDeploymentInput, ApplicationDeploymentResult, CatalogEntry, ComponentAction,
    ComponentInventory, InstallPreview, RetryPreview, SoftwareCenterError, SoftwareCenterService,
    SoftwareDiagnostics, SoftwareJobView,
};
use serde::Deserialize;

use crate::{ApiError, ApiResult, AuthUser};

/// Build `/software` API routes.
pub fn router(service: Arc<SoftwareCenterService>) -> Router {
    Router::new()
        .route("/catalog", get(catalog))
        .route("/inventory", get(inventory))
        .route("/components/{id}/preview", post(preview))
        .route("/components/{id}/{action}/preview", post(preview_component))
        .route("/plans/{digest}/execute", post(execute))
        .route("/applications/preview", post(preview_deployment))
        .route(
            "/applications/plans/{digest}/execute",
            post(execute_deployment),
        )
        .route("/jobs", get(jobs))
        .route("/diagnostics", get(diagnostics))
        .route("/jobs/{id}/cancel", post(cancel))
        .route("/jobs/{id}/retry", post(retry))
        .route("/jobs/{id}/rollback", post(rollback))
        .with_state(service)
}

async fn preview_component(
    State(service): State<Arc<SoftwareCenterService>>,
    AuthUser(user, _): AuthUser,
    Path((id, action)): Path<(String, ComponentAction)>,
) -> ApiResult<Json<InstallPreview>> {
    Ok(Json(
        service
            .preview_component(user.id(), user.role(), &id, action)
            .await
            .map_err(map)?,
    ))
}

async fn preview_deployment(
    State(service): State<Arc<SoftwareCenterService>>,
    AuthUser(user, _): AuthUser,
    Json(input): Json<ApplicationDeploymentInput>,
) -> ApiResult<Json<InstallPreview>> {
    Ok(Json(
        service
            .preview_deployment(user.id(), user.role(), input)
            .await
            .map_err(map)?,
    ))
}

async fn execute_deployment(
    State(service): State<Arc<SoftwareCenterService>>,
    AuthUser(user, _): AuthUser,
    Path(digest): Path<String>,
    Json(input): Json<ExecuteInput>,
) -> ApiResult<Json<ApplicationDeploymentResult>> {
    Ok(Json(
        service
            .execute_deployment(user.id(), user.role(), &digest, &input.confirmation_token)
            .await
            .map_err(map)?,
    ))
}

async fn inventory(
    State(service): State<Arc<SoftwareCenterService>>,
    AuthUser(user, _): AuthUser,
) -> ApiResult<Json<Vec<ComponentInventory>>> {
    Ok(Json(service.inventory(user.role()).await.map_err(map)?))
}

async fn catalog(
    State(service): State<Arc<SoftwareCenterService>>,
    AuthUser(user, _): AuthUser,
) -> ApiResult<Json<Vec<CatalogEntry>>> {
    Ok(Json(service.catalog(user.role()).map_err(map)?))
}

async fn preview(
    State(service): State<Arc<SoftwareCenterService>>,
    AuthUser(user, _): AuthUser,
    Path(id): Path<String>,
) -> ApiResult<Json<InstallPreview>> {
    Ok(Json(
        service
            .preview_install(user.id(), user.role(), &id)
            .await
            .map_err(map)?,
    ))
}

#[derive(Deserialize)]
struct ExecuteInput {
    confirmation_token: String,
}

async fn execute(
    State(service): State<Arc<SoftwareCenterService>>,
    AuthUser(user, _): AuthUser,
    Path(digest): Path<String>,
    Json(input): Json<ExecuteInput>,
) -> ApiResult<Json<SoftwareJobView>> {
    Ok(Json(
        service
            .execute(user.id(), user.role(), &digest, &input.confirmation_token)
            .await
            .map_err(map)?,
    ))
}

async fn jobs(
    State(service): State<Arc<SoftwareCenterService>>,
    AuthUser(user, _): AuthUser,
) -> ApiResult<Json<Vec<SoftwareJobView>>> {
    Ok(Json(service.jobs(user.role()).await.map_err(map)?))
}

async fn diagnostics(
    State(service): State<Arc<SoftwareCenterService>>,
    AuthUser(user, _): AuthUser,
) -> ApiResult<Json<SoftwareDiagnostics>> {
    Ok(Json(service.diagnostics(user.role()).await.map_err(map)?))
}

async fn cancel(
    State(service): State<Arc<SoftwareCenterService>>,
    AuthUser(user, _): AuthUser,
    Path(id): Path<uuid::Uuid>,
) -> ApiResult<Json<SoftwareJobView>> {
    Ok(Json(
        service
            .cancel(user.id(), user.role(), id)
            .await
            .map_err(map)?,
    ))
}

async fn retry(
    State(service): State<Arc<SoftwareCenterService>>,
    AuthUser(user, _): AuthUser,
    Path(id): Path<uuid::Uuid>,
) -> ApiResult<Json<RetryPreview>> {
    Ok(Json(
        service
            .retry_preview(user.id(), user.role(), id)
            .await
            .map_err(map)?,
    ))
}

async fn rollback(
    State(service): State<Arc<SoftwareCenterService>>,
    AuthUser(user, _): AuthUser,
    Path(id): Path<uuid::Uuid>,
) -> ApiResult<Json<SoftwareJobView>> {
    Ok(Json(
        service
            .rollback_interrupted(user.id(), user.role(), id)
            .await
            .map_err(map)?,
    ))
}

fn map(error: SoftwareCenterError) -> ApiError {
    match error {
        SoftwareCenterError::Forbidden => ApiError::Forbidden,
        SoftwareCenterError::Invalid => ApiError::Unprocessable(error.to_string()),
        SoftwareCenterError::Conflict | SoftwareCenterError::Dependencies(_) => {
            ApiError::Conflict(error.to_string())
        }
        SoftwareCenterError::Validation
        | SoftwareCenterError::Package
        | SoftwareCenterError::Repository => ApiError::Internal(error.to_string()),
        SoftwareCenterError::Unsupported => ApiError::Unprocessable(error.to_string()),
    }
}
