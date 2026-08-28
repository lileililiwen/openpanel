//! Backup plan, run, verification, and restore routes.

use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};
use openpanel_app::backups::{
    BackupPlanInput, BackupPlanUpdate, BackupService, BackupServiceError, DrillService,
    DrillServiceError, RestoreInput,
};
use openpanel_domain::{
    Role,
    backups::{BackupPlan, BackupRun, drill::RestoreDrill},
};
use uuid::Uuid;

use crate::{ApiError, ApiResult, AuthUser};

/// Build `/backups` routes.
pub fn router(service: Arc<BackupService>, drills: Arc<DrillService>) -> Router {
    Router::new()
        .route("/plans", get(plans).post(create))
        .route("/plans/{id}", get(plan).put(update).delete(delete_plan))
        .route("/plans/{id}/enable", post(enable))
        .route("/plans/{id}/disable", post(disable))
        .route("/plans/{id}/run", post(run_plan))
        .route("/runs", get(runs))
        .route("/runs/{id}", get(run).delete(delete_run))
        .route("/runs/{id}/verify", post(verify))
        .route("/runs/{id}/restore/preview", post(preview))
        .route("/runs/{id}/restore", post(restore))
        .route("/runs/{id}/drills", post(run_drill).get(list_drills))
        .route("/runs/{id}/drills/{drill_id}", get(show_drill))
        .with_state((service, drills))
}
fn scope(user: &openpanel_domain::User) -> (Uuid, bool) {
    (user.id(), matches!(user.role(), Role::Owner))
}
async fn create(
    State((svc, _drills)): State<(Arc<BackupService>, Arc<DrillService>)>,
    AuthUser(user, _): AuthUser,
    Json(input): Json<BackupPlanInput>,
) -> ApiResult<impl IntoResponse> {
    Ok((
        StatusCode::CREATED,
        Json(svc.create_plan(user.id(), input).await.map_err(map)?),
    ))
}
async fn plans(
    State((svc, _drills)): State<(Arc<BackupService>, Arc<DrillService>)>,
    AuthUser(user, _): AuthUser,
) -> ApiResult<Json<Vec<BackupPlan>>> {
    let (owner, all) = scope(&user);
    Ok(Json(svc.plans(owner, all).await.map_err(map)?))
}
async fn plan(
    State((svc, _drills)): State<(Arc<BackupService>, Arc<DrillService>)>,
    AuthUser(user, _): AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<BackupPlan>> {
    let (owner, all) = scope(&user);
    Ok(Json(svc.plan(owner, all, id).await.map_err(map)?))
}
async fn update(
    State((svc, _drills)): State<(Arc<BackupService>, Arc<DrillService>)>,
    AuthUser(user, _): AuthUser,
    Path(id): Path<Uuid>,
    Json(input): Json<BackupPlanUpdate>,
) -> ApiResult<Json<BackupPlan>> {
    let (owner, all) = scope(&user);
    Ok(Json(
        svc.update_plan(owner, all, id, input).await.map_err(map)?,
    ))
}
async fn enable(
    State((svc, _drills)): State<(Arc<BackupService>, Arc<DrillService>)>,
    AuthUser(user, _): AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<BackupPlan>> {
    status(svc, user, id, true).await
}
async fn disable(
    State((svc, _drills)): State<(Arc<BackupService>, Arc<DrillService>)>,
    AuthUser(user, _): AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<BackupPlan>> {
    status(svc, user, id, false).await
}
async fn status(
    svc: Arc<BackupService>,
    user: openpanel_domain::User,
    id: Uuid,
    enabled: bool,
) -> ApiResult<Json<BackupPlan>> {
    let (owner, all) = scope(&user);
    Ok(Json(
        svc.set_enabled(owner, all, id, enabled)
            .await
            .map_err(map)?,
    ))
}
async fn delete_plan(
    State((svc, _drills)): State<(Arc<BackupService>, Arc<DrillService>)>,
    AuthUser(user, _): AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    let (owner, all) = scope(&user);
    svc.delete_plan(owner, all, id).await.map_err(map)?;
    Ok(StatusCode::NO_CONTENT)
}
async fn run_plan(
    State((svc, _drills)): State<(Arc<BackupService>, Arc<DrillService>)>,
    AuthUser(user, _): AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<impl IntoResponse> {
    let (owner, all) = scope(&user);
    Ok((
        StatusCode::ACCEPTED,
        Json(svc.run_plan(owner, all, id).await.map_err(map)?),
    ))
}
async fn runs(
    State((svc, _drills)): State<(Arc<BackupService>, Arc<DrillService>)>,
    AuthUser(user, _): AuthUser,
) -> ApiResult<Json<Vec<BackupRun>>> {
    let (owner, all) = scope(&user);
    Ok(Json(svc.runs(owner, all).await.map_err(map)?))
}
async fn run(
    State((svc, _drills)): State<(Arc<BackupService>, Arc<DrillService>)>,
    AuthUser(user, _): AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<BackupRun>> {
    let (owner, all) = scope(&user);
    Ok(Json(svc.run(owner, all, id).await.map_err(map)?))
}
async fn verify(
    State((svc, _drills)): State<(Arc<BackupService>, Arc<DrillService>)>,
    AuthUser(user, _): AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<BackupRun>> {
    let (owner, all) = scope(&user);
    Ok(Json(svc.verify(owner, all, id).await.map_err(map)?))
}
async fn preview(
    State((svc, _drills)): State<(Arc<BackupService>, Arc<DrillService>)>,
    AuthUser(user, _): AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<openpanel_app::backups::RestorePreview>> {
    let (owner, all) = scope(&user);
    Ok(Json(
        svc.restore_preview(owner, all, id).await.map_err(map)?,
    ))
}
async fn restore(
    State((svc, _drills)): State<(Arc<BackupService>, Arc<DrillService>)>,
    AuthUser(user, _): AuthUser,
    Path(id): Path<Uuid>,
    Json(input): Json<RestoreInput>,
) -> ApiResult<impl IntoResponse> {
    let (owner, all) = scope(&user);
    Ok((
        StatusCode::ACCEPTED,
        Json(svc.restore(owner, all, id, input).await.map_err(map)?),
    ))
}
async fn delete_run(
    State((svc, _drills)): State<(Arc<BackupService>, Arc<DrillService>)>,
    AuthUser(user, _): AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    let (owner, all) = scope(&user);
    svc.delete_run(owner, all, id).await.map_err(map)?;
    Ok(StatusCode::NO_CONTENT)
}
async fn run_drill(
    State((_svc, drills)): State<(Arc<BackupService>, Arc<DrillService>)>,
    AuthUser(_user, _): AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<impl IntoResponse> {
    Ok((
        StatusCode::ACCEPTED,
        Json(drills.run_drill(id).await.map_err(map_drill)?),
    ))
}
async fn list_drills(
    State((_svc, drills)): State<(Arc<BackupService>, Arc<DrillService>)>,
    AuthUser(_user, _): AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Vec<RestoreDrill>>> {
    Ok(Json(drills.list_drills(id).await.map_err(map_drill)?))
}
async fn show_drill(
    State((_svc, drills)): State<(Arc<BackupService>, Arc<DrillService>)>,
    AuthUser(_user, _): AuthUser,
    Path((_id, drill_id)): Path<(Uuid, Uuid)>,
) -> ApiResult<Json<RestoreDrill>> {
    Ok(Json(drills.get_drill(drill_id).await.map_err(map_drill)?))
}
fn map(error: BackupServiceError) -> ApiError {
    match error {
        BackupServiceError::Validation(message) => ApiError::Unprocessable(message),
        BackupServiceError::NotFound => ApiError::NotFound("backup record".into()),
        BackupServiceError::Corrupt => ApiError::Conflict("backup artifact is corrupt".into()),
        BackupServiceError::Internal(message) => ApiError::Internal(message),
    }
}
fn map_drill(error: DrillServiceError) -> ApiError {
    match error {
        DrillServiceError::Validation(message) => ApiError::Unprocessable(message),
        DrillServiceError::NotFound => ApiError::NotFound("drill record".into()),
        DrillServiceError::RunNotFound => ApiError::NotFound("backup run".into()),
        DrillServiceError::Internal(message) => ApiError::Internal(message),
    }
}
