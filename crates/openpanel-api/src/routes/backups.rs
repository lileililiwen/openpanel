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
    BackupPlanInput, BackupPlanUpdate, BackupService, BackupServiceError, RestoreInput,
};
use openpanel_domain::{
    Role,
    backups::{BackupPlan, BackupRun},
};
use uuid::Uuid;

use crate::{ApiError, ApiResult, AuthUser};

/// Build `/backups` routes.
pub fn router(service: Arc<BackupService>) -> Router {
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
        .with_state(service)
}
fn scope(user: &openpanel_domain::User) -> (Uuid, bool) {
    (user.id(), matches!(user.role(), Role::Owner))
}
async fn create(
    State(svc): State<Arc<BackupService>>,
    AuthUser(user, _): AuthUser,
    Json(input): Json<BackupPlanInput>,
) -> ApiResult<impl IntoResponse> {
    Ok((
        StatusCode::CREATED,
        Json(svc.create_plan(user.id(), input).await.map_err(map)?),
    ))
}
async fn plans(
    State(svc): State<Arc<BackupService>>,
    AuthUser(user, _): AuthUser,
) -> ApiResult<Json<Vec<BackupPlan>>> {
    let (owner, all) = scope(&user);
    Ok(Json(svc.plans(owner, all).await.map_err(map)?))
}
async fn plan(
    State(svc): State<Arc<BackupService>>,
    AuthUser(user, _): AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<BackupPlan>> {
    let (owner, all) = scope(&user);
    Ok(Json(svc.plan(owner, all, id).await.map_err(map)?))
}
async fn update(
    State(svc): State<Arc<BackupService>>,
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
    State(svc): State<Arc<BackupService>>,
    AuthUser(user, _): AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<BackupPlan>> {
    status(svc, user, id, true).await
}
async fn disable(
    State(svc): State<Arc<BackupService>>,
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
    State(svc): State<Arc<BackupService>>,
    AuthUser(user, _): AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    let (owner, all) = scope(&user);
    svc.delete_plan(owner, all, id).await.map_err(map)?;
    Ok(StatusCode::NO_CONTENT)
}
async fn run_plan(
    State(svc): State<Arc<BackupService>>,
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
    State(svc): State<Arc<BackupService>>,
    AuthUser(user, _): AuthUser,
) -> ApiResult<Json<Vec<BackupRun>>> {
    let (owner, all) = scope(&user);
    Ok(Json(svc.runs(owner, all).await.map_err(map)?))
}
async fn run(
    State(svc): State<Arc<BackupService>>,
    AuthUser(user, _): AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<BackupRun>> {
    let (owner, all) = scope(&user);
    Ok(Json(svc.run(owner, all, id).await.map_err(map)?))
}
async fn verify(
    State(svc): State<Arc<BackupService>>,
    AuthUser(user, _): AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<BackupRun>> {
    let (owner, all) = scope(&user);
    Ok(Json(svc.verify(owner, all, id).await.map_err(map)?))
}
async fn preview(
    State(svc): State<Arc<BackupService>>,
    AuthUser(user, _): AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<openpanel_app::backups::RestorePreview>> {
    let (owner, all) = scope(&user);
    Ok(Json(
        svc.restore_preview(owner, all, id).await.map_err(map)?,
    ))
}
async fn restore(
    State(svc): State<Arc<BackupService>>,
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
    State(svc): State<Arc<BackupService>>,
    AuthUser(user, _): AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    let (owner, all) = scope(&user);
    svc.delete_run(owner, all, id).await.map_err(map)?;
    Ok(StatusCode::NO_CONTENT)
}
fn map(error: BackupServiceError) -> ApiError {
    match error {
        BackupServiceError::Validation(message) => ApiError::Unprocessable(message),
        BackupServiceError::NotFound => ApiError::NotFound("backup record".into()),
        BackupServiceError::Corrupt => ApiError::Conflict("backup artifact is corrupt".into()),
        BackupServiceError::Internal(message) => ApiError::Internal(message),
    }
}
