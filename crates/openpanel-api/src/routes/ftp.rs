//! Per-site FTP account management API.

use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::get,
};
use openpanel_app::{
    CreateFtpAccount, CreatedFtpAccount, FtpAccountView, FtpService, UpdateFtpAccount,
};
use openpanel_domain::ftp::{FtpError, FtpSession};
use uuid::Uuid;

use crate::{ApiError, ApiResult, AuthUser};

/// Build routes nested under `/sites`.
pub fn router(service: Arc<FtpService>) -> Router {
    Router::new()
        .route("/{site_id}/ftp/accounts", get(list).post(create))
        .route(
            "/{site_id}/ftp/accounts/{account_id}",
            axum::routing::patch(update).delete(delete),
        )
        .route(
            "/{site_id}/ftp/accounts/{account_id}/sessions",
            get(sessions),
        )
        .with_state(service)
}

async fn list(
    State(service): State<Arc<FtpService>>,
    AuthUser(user, _): AuthUser,
    Path(site_id): Path<Uuid>,
) -> ApiResult<Json<Vec<FtpAccountView>>> {
    Ok(Json(service.list(&user, site_id).await.map_err(map)?))
}
async fn create(
    State(service): State<Arc<FtpService>>,
    AuthUser(user, _): AuthUser,
    Path(site_id): Path<Uuid>,
    Json(input): Json<CreateFtpAccount>,
) -> ApiResult<(StatusCode, Json<CreatedFtpAccount>)> {
    Ok((
        StatusCode::CREATED,
        Json(service.create(&user, site_id, input).await.map_err(map)?),
    ))
}
async fn update(
    State(service): State<Arc<FtpService>>,
    AuthUser(user, _): AuthUser,
    Path((site_id, account_id)): Path<(Uuid, Uuid)>,
    Json(input): Json<UpdateFtpAccount>,
) -> ApiResult<Json<FtpAccountView>> {
    Ok(Json(
        service
            .update(&user, site_id, account_id, input)
            .await
            .map_err(map)?,
    ))
}
async fn delete(
    State(service): State<Arc<FtpService>>,
    AuthUser(user, _): AuthUser,
    Path((site_id, account_id)): Path<(Uuid, Uuid)>,
) -> ApiResult<StatusCode> {
    service
        .delete(&user, site_id, account_id)
        .await
        .map_err(map)?;
    Ok(StatusCode::NO_CONTENT)
}
async fn sessions(
    State(service): State<Arc<FtpService>>,
    AuthUser(user, _): AuthUser,
    Path((site_id, account_id)): Path<(Uuid, Uuid)>,
) -> ApiResult<Json<Vec<FtpSession>>> {
    Ok(Json(
        service
            .sessions(&user, site_id, account_id)
            .await
            .map_err(map)?,
    ))
}

fn map(error: FtpError) -> ApiError {
    match error {
        FtpError::Forbidden | FtpError::LoginDenied | FtpError::OperationDenied => {
            ApiError::Forbidden
        }
        FtpError::SiteNotFound | FtpError::NotFound => ApiError::NotFound(error.to_string()),
        FtpError::Duplicate => ApiError::Conflict(error.to_string()),
        FtpError::Invalid(message) => ApiError::Unprocessable(message),
        FtpError::ConnectionLimit => ApiError::Conflict(error.to_string()),
        FtpError::Internal(message) => ApiError::Internal(message),
    }
}
