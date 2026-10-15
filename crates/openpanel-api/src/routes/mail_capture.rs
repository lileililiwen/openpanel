//! Staging mail-capture inbox API.

use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, State},
    routing::get,
};
use openpanel_app::{MailCaptureService, MailCaptureView, ReadCapturedMessageView};
use openpanel_domain::MailCaptureError;
use uuid::Uuid;

use crate::{ApiError, ApiResult, AuthUser};

/// Build routes nested under `/sites`.
pub fn router(service: Arc<MailCaptureService>) -> Router {
    Router::new()
        .route("/{site_id}/mail-capture", get(list))
        .route("/{site_id}/mail-capture/{message_id}", get(read))
        .route(
            "/{site_id}/mail-capture/purge",
            axum::routing::post(purge_site),
        )
        .with_state(service)
}

async fn list(
    State(service): State<Arc<MailCaptureService>>,
    AuthUser(user, _): AuthUser,
    Path(site_id): Path<Uuid>,
) -> ApiResult<Json<Vec<MailCaptureView>>> {
    Ok(Json(service.list(&user, site_id).await.map_err(map)?))
}

async fn read(
    State(service): State<Arc<MailCaptureService>>,
    AuthUser(user, _): AuthUser,
    Path((site_id, message_id)): Path<(Uuid, Uuid)>,
) -> ApiResult<Json<ReadCapturedMessageView>> {
    Ok(Json(
        service
            .read(&user, site_id, message_id)
            .await
            .map_err(map)?,
    ))
}

async fn purge_site(
    State(service): State<Arc<MailCaptureService>>,
    AuthUser(user, _): AuthUser,
    Path(site_id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    let removed = service.purge(&user, site_id).await.map_err(map)?;
    Ok(Json(serde_json::json!({"removed": removed})))
}

fn map(error: MailCaptureError) -> ApiError {
    match error {
        MailCaptureError::Forbidden => ApiError::Forbidden,
        MailCaptureError::SiteNotFound | MailCaptureError::NotFound => {
            ApiError::NotFound(error.to_string())
        }
        MailCaptureError::Invalid(message) => ApiError::Unprocessable(message),
        MailCaptureError::Internal(message) => ApiError::Internal(message),
    }
}
