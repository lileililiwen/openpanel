//! Per-site staging HTTP routes.
//!
//! Mounted under `/api/v1/sites/{id}/staging/...`:
//!
//! * `POST   /sites/{id}/staging`     — create slot
//! * `DELETE /sites/{id}/staging`     — destroy slot
//! * `POST   /sites/{id}/staging/sync`— take a snapshot
//! * `POST   /sites/{id}/staging/promote` — promote to live
//! * `GET    /sites/{id}/staging`     — read slot + recent promotions

use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};
use chrono::{DateTime, Utc};
use openpanel_app::site_staging::{CreateSlotRequest, PromotionServiceError, StagingService};
use openpanel_domain::{PromotionRun, PromotionStatus, StagingSlot, SyncMode, SyncPolicy};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{ApiError, ApiResult, AuthUser};

/// Build the staging routes.
pub fn router(service: Arc<StagingService>) -> Router {
    Router::new()
        .route(
            "/{id}/staging",
            get(get_slot).post(create_slot).delete(delete_slot),
        )
        .route("/{id}/staging/sync", post(sync))
        .route("/{id}/staging/promote", post(promote))
        .with_state(service)
}

async fn get_slot(
    State(svc): State<Arc<StagingService>>,
    AuthUser(user, _): AuthUser,
    Path(site_id): Path<Uuid>,
) -> ApiResult<Json<StagingView>> {
    let slot = svc.slot_for(&user, site_id).await.map_err(map)?;
    let promotions = svc.promotions(&user, site_id).await.map_err(map)?;
    Ok(Json(StagingView::new(slot, promotions)))
}

async fn create_slot(
    State(svc): State<Arc<StagingService>>,
    AuthUser(user, _): AuthUser,
    Path(site_id): Path<Uuid>,
    Json(body): Json<CreateSlotBody>,
) -> ApiResult<impl IntoResponse> {
    let req = CreateSlotRequest {
        subdomain: body.subdomain,
        document_root: body.document_root,
        sync_policy: body.sync_policy,
        php_version: body.php_version,
        schedule: body.schedule,
    };
    let slot = svc.create_slot(&user, site_id, req).await.map_err(map)?;
    Ok((StatusCode::CREATED, Json(StagingSlotView::from(slot))))
}

#[derive(Debug, Deserialize)]
struct CreateSlotBody {
    #[serde(default)]
    subdomain: Option<String>,
    #[serde(default)]
    document_root: Option<String>,
    #[serde(default)]
    sync_policy: Option<SyncPolicy>,
    #[serde(default)]
    php_version: Option<String>,
    #[serde(default)]
    schedule: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DeleteSlotBody {
    #[serde(default)]
    drop_db: bool,
}

async fn delete_slot(
    State(svc): State<Arc<StagingService>>,
    AuthUser(user, _): AuthUser,
    Path(site_id): Path<Uuid>,
    Json(body): Json<DeleteSlotBody>,
) -> ApiResult<StatusCode> {
    svc.delete_slot(&user, site_id, body.drop_db)
        .await
        .map_err(map)?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
struct SyncBody {
    // Accepted for forward compatibility; not yet read by `sync`.
    #[serde(default)]
    #[allow(dead_code)]
    mode: Option<SyncMode>,
}

async fn sync(
    State(svc): State<Arc<StagingService>>,
    AuthUser(user, _): AuthUser,
    Path(site_id): Path<Uuid>,
    Json(_body): Json<SyncBody>,
) -> ApiResult<Json<StagingSlotView>> {
    let slot = svc.sync_snapshot(&user, site_id).await.map_err(map)?;
    Ok(Json(StagingSlotView::from(slot)))
}

#[derive(Debug, Deserialize)]
struct PromoteBody {
    snapshot: i64,
    confirmed_at: DateTime<Utc>,
}

async fn promote(
    State(svc): State<Arc<StagingService>>,
    AuthUser(user, _): AuthUser,
    Path(site_id): Path<Uuid>,
    Json(body): Json<PromoteBody>,
) -> ApiResult<Json<PromotionRunView>> {
    let snapshot = openpanel_domain::SnapshotId::new(body.snapshot)
        .map_err(|e| ApiError::Unprocessable(e.to_string()))?;
    let run = svc
        .promote(&user, site_id, snapshot, body.confirmed_at)
        .await
        .map_err(map)?;
    Ok(Json(PromotionRunView::from(run)))
}

#[derive(Debug, Serialize)]
struct StagingView {
    slot: Option<StagingSlotView>,
    promotions: Vec<PromotionRunView>,
}

impl StagingView {
    fn new(slot: Option<StagingSlot>, promotions: Vec<PromotionRun>) -> Self {
        Self {
            slot: slot.map(StagingSlotView::from),
            promotions: promotions.into_iter().map(PromotionRunView::from).collect(),
        }
    }
}

#[derive(Debug, Serialize)]
struct StagingSlotView {
    id: Uuid,
    site_id: Uuid,
    subdomain: String,
    document_root: String,
    db_name: String,
    php_version: Option<String>,
    sync_policy: String,
    schedule: Option<String>,
    current_snapshot: Option<i64>,
    last_promoted_snapshot: Option<i64>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<StagingSlot> for StagingSlotView {
    fn from(s: StagingSlot) -> Self {
        Self {
            id: s.id(),
            site_id: s.site_id(),
            subdomain: s.subdomain().to_string(),
            document_root: s.document_root().to_string(),
            db_name: s.db_name().to_string(),
            php_version: s.php_version().map(str::to_owned),
            sync_policy: s.sync_policy().as_str().to_string(),
            schedule: s.schedule().map(str::to_owned),
            current_snapshot: s.current_snapshot().map(|s| s.as_i64()),
            last_promoted_snapshot: s.last_promoted_snapshot().map(|s| s.as_i64()),
            created_at: s.created_at(),
            updated_at: s.updated_at(),
        }
    }
}

#[derive(Debug, Serialize)]
struct PromotionRunView {
    id: Uuid,
    site_id: Uuid,
    snapshot: i64,
    status: String,
    requested_by: String,
    requested_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    failure_reason: Option<String>,
}

impl From<PromotionRun> for PromotionRunView {
    fn from(r: PromotionRun) -> Self {
        Self {
            id: r.id(),
            site_id: r.site_id(),
            snapshot: r.snapshot().as_i64(),
            status: match r.status() {
                PromotionStatus::Pending => "pending",
                PromotionStatus::Promoting => "promoting",
                PromotionStatus::Promoted => "promoted",
                PromotionStatus::RolledBack => "rolled_back",
                PromotionStatus::Failed => "failed",
            }
            .to_string(),
            requested_by: r.requested_by().to_string(),
            requested_at: r.requested_at(),
            updated_at: r.updated_at(),
            failure_reason: r.failure_reason().map(str::to_owned),
        }
    }
}

fn map(error: PromotionServiceError) -> ApiError {
    match error {
        PromotionServiceError::Forbidden => ApiError::Forbidden,
        PromotionServiceError::SiteNotFound(_) => ApiError::NotFound("site not found".into()),
        PromotionServiceError::Staging(e) => match e {
            openpanel_domain::SiteStagingError::NotFound(_) => {
                ApiError::NotFound("staging slot not found".into())
            }
            openpanel_domain::SiteStagingError::AlreadyExists(_) => {
                ApiError::Conflict("staging slot already exists".into())
            }
            openpanel_domain::SiteStagingError::InFlight => {
                ApiError::Conflict("sync or promote is already in flight".into())
            }
            openpanel_domain::SiteStagingError::OutsideChroot(msg) => {
                ApiError::Unprocessable(format!("document root `{msg}` is outside the site chroot"))
            }
            other => ApiError::Unprocessable(other.to_string()),
        },
    }
}
