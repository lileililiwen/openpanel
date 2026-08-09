//! Cron job and execution-history HTTP routes.

use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};
use openpanel_app::cron::{CronInput, CronService, CronServiceError, CronUpdate};
use openpanel_domain::{
    Role,
    cron::{CronJob, JobRun},
};
use serde::Deserialize;
use uuid::Uuid;

use crate::{ApiError, ApiResult, AuthUser};

/// Build the `/cron` API sub-router.
pub fn router(service: Arc<CronService>) -> Router {
    Router::new()
        .route("/jobs", get(list).post(create))
        .route("/jobs/{id}", get(get_one).put(update).delete(delete))
        .route("/jobs/{id}/enable", post(enable))
        .route("/jobs/{id}/disable", post(disable))
        .route("/jobs/{id}/run", post(run_now))
        .route("/runs", get(runs))
        .route("/runs/{id}", get(run_detail))
        .with_state(service)
}

fn scope(user: &openpanel_domain::User) -> (Uuid, bool) {
    (user.id(), matches!(user.role(), Role::Owner))
}

async fn create(
    State(svc): State<Arc<CronService>>,
    AuthUser(user, _): AuthUser,
    Json(input): Json<CronInput>,
) -> ApiResult<impl IntoResponse> {
    let job = svc.create(user.id(), input).await.map_err(map)?;
    Ok((StatusCode::CREATED, Json(job)))
}
async fn list(
    State(svc): State<Arc<CronService>>,
    AuthUser(user, _): AuthUser,
) -> ApiResult<Json<Vec<CronJob>>> {
    let (id, all) = scope(&user);
    Ok(Json(svc.list(id, all).await.map_err(map)?))
}
async fn get_one(
    State(svc): State<Arc<CronService>>,
    AuthUser(user, _): AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<CronJob>> {
    let (owner, all) = scope(&user);
    Ok(Json(svc.get(owner, all, id).await.map_err(map)?))
}
async fn update(
    State(svc): State<Arc<CronService>>,
    AuthUser(user, _): AuthUser,
    Path(id): Path<Uuid>,
    Json(input): Json<CronUpdate>,
) -> ApiResult<Json<CronJob>> {
    let (owner, all) = scope(&user);
    Ok(Json(svc.update(owner, all, id, input).await.map_err(map)?))
}
async fn enable(
    State(svc): State<Arc<CronService>>,
    AuthUser(user, _): AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<CronJob>> {
    set_enabled(svc, user, id, true).await
}
async fn disable(
    State(svc): State<Arc<CronService>>,
    AuthUser(user, _): AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<CronJob>> {
    set_enabled(svc, user, id, false).await
}
async fn set_enabled(
    svc: Arc<CronService>,
    user: openpanel_domain::User,
    id: Uuid,
    enabled: bool,
) -> ApiResult<Json<CronJob>> {
    let (owner, all) = scope(&user);
    Ok(Json(
        svc.set_enabled(owner, all, id, enabled)
            .await
            .map_err(map)?,
    ))
}
async fn delete(
    State(svc): State<Arc<CronService>>,
    AuthUser(user, _): AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    let (owner, all) = scope(&user);
    svc.delete(owner, all, id).await.map_err(map)?;
    Ok(StatusCode::NO_CONTENT)
}
async fn run_now(
    State(svc): State<Arc<CronService>>,
    AuthUser(user, _): AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<impl IntoResponse> {
    let (owner, all) = scope(&user);
    let run = svc.run_now(owner, all, id).await.map_err(map)?;
    Ok((StatusCode::ACCEPTED, Json(run)))
}

#[derive(Deserialize)]
struct RunQuery {
    job_id: Option<Uuid>,
}
async fn runs(
    State(svc): State<Arc<CronService>>,
    AuthUser(user, _): AuthUser,
    Query(query): Query<RunQuery>,
) -> ApiResult<Json<Vec<JobRun>>> {
    let (owner, all) = scope(&user);
    Ok(Json(svc.runs(owner, all, query.job_id).await.map_err(map)?))
}
async fn run_detail(
    State(svc): State<Arc<CronService>>,
    AuthUser(user, _): AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<JobRun>> {
    let (owner, all) = scope(&user);
    Ok(Json(svc.get_run(owner, all, id).await.map_err(map)?))
}

fn map(error: CronServiceError) -> ApiError {
    match error {
        CronServiceError::Validation(message) => ApiError::Unprocessable(message),
        CronServiceError::NotFound => ApiError::NotFound("cron record".into()),
        CronServiceError::Persistence(message) => ApiError::Internal(message),
    }
}
