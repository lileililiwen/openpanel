//! Per-site WebDAV account management API.

use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::get,
};
use openpanel_app::{
    CreateWebDavAccount, CreatedWebDavAccount, UpdateWebDavAccount, WebDavAccountView,
    WebDavService,
};
use openpanel_domain::webdav::WebDavError;
use uuid::Uuid;

use crate::{ApiError, ApiResult, AuthUser};

/// Build routes nested under `/sites`.
pub fn router(service: Arc<WebDavService>) -> Router {
    Router::new()
        .route("/{site_id}/webdav/accounts", get(list).post(create))
        .route(
            "/{site_id}/webdav/accounts/{account_id}",
            axum::routing::patch(update).delete(delete),
        )
        .with_state(service)
}

async fn list(
    State(service): State<Arc<WebDavService>>,
    AuthUser(user, _): AuthUser,
    Path(site_id): Path<Uuid>,
) -> ApiResult<Json<Vec<WebDavAccountView>>> {
    Ok(Json(service.list(&user, site_id).await.map_err(map)?))
}
async fn create(
    State(service): State<Arc<WebDavService>>,
    AuthUser(user, _): AuthUser,
    Path(site_id): Path<Uuid>,
    Json(input): Json<CreateWebDavAccount>,
) -> ApiResult<(StatusCode, Json<CreatedWebDavAccount>)> {
    Ok((
        StatusCode::CREATED,
        Json(service.create(&user, site_id, input).await.map_err(map)?),
    ))
}
async fn update(
    State(service): State<Arc<WebDavService>>,
    AuthUser(user, _): AuthUser,
    Path((site_id, account_id)): Path<(Uuid, Uuid)>,
    Json(input): Json<UpdateWebDavAccount>,
) -> ApiResult<Json<WebDavAccountView>> {
    Ok(Json(
        service
            .update(&user, site_id, account_id, input)
            .await
            .map_err(map)?,
    ))
}
async fn delete(
    State(service): State<Arc<WebDavService>>,
    AuthUser(user, _): AuthUser,
    Path((site_id, account_id)): Path<(Uuid, Uuid)>,
) -> ApiResult<StatusCode> {
    service
        .delete(&user, site_id, account_id)
        .await
        .map_err(map)?;
    Ok(StatusCode::NO_CONTENT)
}

fn map(error: WebDavError) -> ApiError {
    match error {
        WebDavError::Forbidden | WebDavError::LoginDenied | WebDavError::OperationDenied => {
            ApiError::Forbidden
        }
        WebDavError::SiteNotFound | WebDavError::NotFound | WebDavError::OutsideJail => {
            ApiError::NotFound(error.to_string())
        }
        WebDavError::Duplicate => ApiError::Conflict(error.to_string()),
        WebDavError::Invalid(message) => ApiError::Unprocessable(message),
        WebDavError::Internal(message) => ApiError::Internal(message),
    }
}
