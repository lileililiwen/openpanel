//! Owner-only Software Center catalog, preview, execution, and job routes.

use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    routing::{get, post},
};
use openpanel_app::software_center::{
    ApplicationDeploymentInput, ApplicationDeploymentResult, CatalogDiagnostics, CatalogEntry,
    CatalogQuery, CatalogSearchPage, ComponentAction, ComponentConfigDocument, ComponentInventory,
    InstallPreview, RefreshOutcome, RetryPreview, SoftwareCenterError, SoftwareCenterService,
    SoftwareJobView, StorefrontEntry,
};
use serde::Deserialize;

use crate::{ApiError, ApiResult, AuthUser};

#[derive(Deserialize, Default)]
#[serde(default)]
struct SearchParams {
    q: Option<String>,
    page: Option<usize>,
    page_size: Option<usize>,
    category: Option<String>,
    tag: Option<String>,
    installed_only: Option<bool>,
    update_available_only: Option<bool>,
    sort: Option<String>,
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
    Ok(Json(service.catalog(user.role()).await.map_err(map)?))
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

async fn diagnostics_full(
    State(service): State<Arc<SoftwareCenterService>>,
    AuthUser(user, _): AuthUser,
) -> ApiResult<Json<ExtendedDiagnostics>> {
    let software = service
        .catalog_diagnostics(user.role())
        .await
        .map_err(map)?;
    let jobs = service.jobs(user.role()).await.map_err(map)?;
    Ok(Json(ExtendedDiagnostics { software, jobs }))
}

/// Combined diagnostics view used by the CLI and the UI strip.
#[derive(serde::Serialize)]
pub struct ExtendedDiagnostics {
    /// Catalog-level diagnostics.
    pub software: CatalogDiagnostics,
    /// Recent job projections.
    pub jobs: Vec<SoftwareJobView>,
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

/// Read the curated config file for a panel-managed component.
async fn get_config(
    State(service): State<Arc<SoftwareCenterService>>,
    AuthUser(user, _): AuthUser,
    Path(id): Path<String>,
) -> ApiResult<Json<ComponentConfigDocument>> {
    Ok(Json(
        service.read_config(user.role(), &id).await.map_err(map)?,
    ))
}

#[derive(Deserialize)]
struct SetConfigInput {
    content: String,
}

/// Save the curated config file for a panel-managed component. Returns
/// the absolute path written; a failed validation restores the previous
/// content and surfaces the error.
async fn set_config(
    State(service): State<Arc<SoftwareCenterService>>,
    AuthUser(user, _): AuthUser,
    Path(id): Path<String>,
    Json(input): Json<SetConfigInput>,
) -> ApiResult<Json<serde_json::Value>> {
    let path = service
        .save_config(user.role(), &id, input.content)
        .await
        .map_err(map)?;
    Ok(Json(serde_json::json!({ "path": path })))
}

fn map(error: SoftwareCenterError) -> ApiError {
    match error {
        SoftwareCenterError::Forbidden => ApiError::Forbidden,
        SoftwareCenterError::Invalid(_) => ApiError::Unprocessable(error.to_string()),
        SoftwareCenterError::Conflict | SoftwareCenterError::Dependencies(_) => {
            ApiError::Conflict(error.to_string())
        }
        SoftwareCenterError::Validation
        | SoftwareCenterError::Package(_)
        | SoftwareCenterError::Repository => ApiError::Internal(error.to_string()),
        SoftwareCenterError::Unsupported => ApiError::Unprocessable(error.to_string()),
    }
}

// ---------------------------------------------------------------------------
// Aggregator routes — search, entry, refresh, extended diagnostics.
// ---------------------------------------------------------------------------

/// Build the `/software` API routes including the new aggregator routes.
pub fn router(service: Arc<SoftwareCenterService>) -> Router {
    Router::new()
        .route("/catalog", get(catalog))
        .route("/inventory", get(inventory))
        .route("/search", get(search))
        .route("/entries/{id}", get(entry))
        .route("/components/{id}/preview", post(preview))
        .route("/components/{id}/config", get(get_config))
        .route("/components/{id}/config", post(set_config))
        .route("/components/{id}/{action}/preview", post(preview_component))
        .route("/plans/{digest}/execute", post(execute))
        .route("/applications/preview", post(preview_deployment))
        .route(
            "/applications/plans/{digest}/execute",
            post(execute_deployment),
        )
        .route("/jobs", get(jobs))
        .route("/diagnostics", get(diagnostics_full))
        .route("/refresh", post(refresh))
        .route("/jobs/{id}/cancel", post(cancel))
        .route("/jobs/{id}/retry", post(retry))
        .route("/jobs/{id}/rollback", post(rollback))
        .with_state(service)
}

async fn search(
    State(service): State<Arc<SoftwareCenterService>>,
    AuthUser(user, _): AuthUser,
    Query(params): Query<SearchParams>,
) -> ApiResult<Json<CatalogSearchPage>> {
    let mut query = CatalogQuery {
        text: params.q,
        page: params.page.unwrap_or(0),
        page_size: params.page_size.unwrap_or(60),
        installed_only: params.installed_only.unwrap_or(false),
        update_available_only: params.update_available_only.unwrap_or(false),
        ..CatalogQuery::default()
    };
    if let Some(category) = params.category
        && let Ok(value) = category.parse::<openpanel_domain::software_center::Category>()
    {
        query.categories.push(value);
    }
    if let Some(tag) = params.tag
        && let Ok(value) = openpanel_domain::software_center::Tag::new(&tag)
    {
        query.tags.push(value);
    }
    Ok(Json(service.search(user.role(), query).await.map_err(map)?))
}

async fn entry(
    State(service): State<Arc<SoftwareCenterService>>,
    AuthUser(user, _): AuthUser,
    Path(id): Path<String>,
) -> ApiResult<Json<Option<StorefrontEntry>>> {
    Ok(Json(service.entry(user.role(), &id).await.map_err(map)?))
}

async fn refresh(
    State(service): State<Arc<SoftwareCenterService>>,
    AuthUser(user, _): AuthUser,
) -> ApiResult<Json<RefreshOutcome>> {
    Ok(Json(
        service
            .refresh_catalog(user.id(), user.role())
            .await
            .map_err(map)?,
    ))
}

// ---------------------------------------------------------------------------
// Aggregator routes — search, entry, refresh, extended diagnostics.
// ---------------------------------------------------------------------------
