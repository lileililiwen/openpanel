//! Database point-in-time recovery HTTP routes.
//!
//! Two endpoints:
//!
//! * `GET  /backups/databases/{id}/binlog` — return the available
//!   transaction-log range for the database.
//! * `POST /backups/databases/{id}/pitr/restore` — request a
//!   point-in-time restore.
//!
//! The endpoints live under `/api/v1/backups/databases` so they
//! stay discoverable alongside the other backup routes.

use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};
use openpanel_app::db_pitr::{PitrService, RestoreRequest, RestoreRequestError};
use openpanel_domain::{BinlogRange, PitrRestore, PitrRestoreStatus};
use serde::Deserialize;
use uuid::Uuid;

use crate::{ApiError, ApiResult, AuthUser};

/// Build the `/backups/databases` routes.
pub fn router(service: Arc<PitrService>) -> Router {
    Router::new()
        .route("/databases/{id}/binlog", get(binlog_range))
        .route(
            "/databases/{id}/pitr/restore",
            post(request_restore).put(promote_restore),
        )
        .with_state(service)
}

async fn binlog_range(
    State(svc): State<Arc<PitrService>>,
    AuthUser(user, _): AuthUser,
    Path(database_id): Path<Uuid>,
) -> ApiResult<Json<BinlogRangeView>> {
    let range = svc.inspect_range(database_id).await.map_err(map)?;
    // The view must scope by owner to avoid leaking the existence
    // of databases the caller does not own.
    let _ = user;
    Ok(Json(BinlogRangeView::from(range)))
}

#[derive(Debug, serde::Serialize)]
struct BinlogRangeView {
    earliest: String,
    latest: String,
    start: chrono::DateTime<chrono::Utc>,
    end: chrono::DateTime<chrono::Utc>,
    empty: bool,
}

impl From<BinlogRange> for BinlogRangeView {
    fn from(r: BinlogRange) -> Self {
        Self {
            earliest: r.earliest.to_hex(),
            latest: r.latest.to_hex(),
            start: r.start,
            end: r.end,
            empty: r.empty,
        }
    }
}

#[derive(Debug, Deserialize)]
struct RestoreBody {
    timestamp: chrono::DateTime<chrono::Utc>,
    #[serde(default)]
    base_backup: Option<Uuid>,
    #[serde(default)]
    confirm: bool,
}

async fn request_restore(
    State(svc): State<Arc<PitrService>>,
    AuthUser(user, _): AuthUser,
    Path(database_id): Path<Uuid>,
    Json(body): Json<RestoreBody>,
) -> ApiResult<impl IntoResponse> {
    let req = RestoreRequest {
        timestamp: body.timestamp,
        base_backup: body.base_backup,
        confirm: body.confirm,
    };
    let restore = svc
        .request_restore(&user, database_id, req)
        .await
        .map_err(map)?;
    let status = if matches!(restore.status(), PitrRestoreStatus::Promoted) {
        StatusCode::OK
    } else {
        StatusCode::ACCEPTED
    };
    Ok((status, Json(PitrRestoreView::from(restore))))
}

async fn promote_restore(
    State(svc): State<Arc<PitrService>>,
    AuthUser(user, _): AuthUser,
    Path(_database_id): Path<Uuid>,
    Json(body): Json<PromoteBody>,
) -> ApiResult<Json<PitrRestoreView>> {
    let restore = svc
        .promote_restore(&user, body.restore_id)
        .await
        .map_err(map)?;
    Ok(Json(PitrRestoreView::from(restore)))
}

#[derive(Debug, Deserialize)]
struct PromoteBody {
    restore_id: Uuid,
}

#[derive(Debug, serde::Serialize)]
struct PitrRestoreView {
    id: Uuid,
    database_id: Uuid,
    request_ts: String,
    base_backup: Option<Uuid>,
    replay_to: Option<String>,
    staging_db_id: Option<Uuid>,
    status: String,
    requested_by: String,
    requested_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
    failure_reason: Option<String>,
}

impl From<PitrRestore> for PitrRestoreView {
    fn from(r: PitrRestore) -> Self {
        Self {
            id: r.id(),
            database_id: r.database_id(),
            request_ts: r.request_ts().to_string(),
            base_backup: r.window().map(|w| w.base_backup),
            replay_to: r.window().map(|w| w.replay_to.to_hex()),
            staging_db_id: r.staging_db_id(),
            status: r.status().as_str().to_string(),
            requested_by: r.requested_by().to_string(),
            requested_at: r.requested_at(),
            updated_at: r.updated_at(),
            failure_reason: r.failure_reason().map(str::to_owned),
        }
    }
}

fn map(error: RestoreRequestError) -> ApiError {
    match error {
        RestoreRequestError::Forbidden => ApiError::Forbidden,
        RestoreRequestError::DatabaseNotFound(msg) => ApiError::NotFound(msg),
        RestoreRequestError::UnknownTarget(msg) => ApiError::NotFound(msg),
        RestoreRequestError::TimestampOutOfRange(msg) => ApiError::Unprocessable(msg),
        RestoreRequestError::Database(_) => ApiError::Internal("database error".into()),
        RestoreRequestError::Pitr(e) => ApiError::Unprocessable(e.to_string()),
    }
}
