//! Whole-server snapshot routes (Owner only).

use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{get, post},
};
use openpanel_app::{ServerSnapshotService, backups::server_snapshot::ServerSnapshotError};
use openpanel_domain::backups::snapshot::SnapshotManifest;
use serde::Deserialize;
use uuid::Uuid;

use crate::{ApiError, ApiResult, AuthUser};

/// Build `/server/snapshots` routes.
pub fn router(service: Arc<ServerSnapshotService>) -> Router {
    Router::new()
        .route("/snapshots", post(create).get(list))
        .route("/snapshots/{id}", get(get_snapshot))
        .route("/snapshots/{id}/preflight", post(preflight))
        .route("/snapshots/{id}/restore", post(restore))
        .with_state(service)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateInput {
    run_id: Uuid,
}

async fn create(
    State(service): State<Arc<ServerSnapshotService>>,
    AuthUser(user, _): AuthUser,
    Json(input): Json<CreateInput>,
) -> ApiResult<Json<SnapshotManifest>> {
    let manifest = service
        .create_from_run(&user, input.run_id)
        .await
        .map_err(map)?;
    Ok(Json(manifest))
}

async fn list(
    State(service): State<Arc<ServerSnapshotService>>,
    AuthUser(_user, _): AuthUser,
) -> ApiResult<Json<serde_json::Value>> {
    let snapshots = service.list().await.map_err(map)?;
    let items: Vec<serde_json::Value> = snapshots
        .into_iter()
        .map(|(id, manifest)| {
            serde_json::json!({
                "id": id.to_string(),
                "created_at": manifest.created_at.to_rfc3339(),
                "source_host_id": manifest.source_host_id.to_string(),
                "panel_version": manifest.panel_version,
                "entries": manifest.entries.len(),
            })
        })
        .collect();
    Ok(Json(serde_json::json!({ "snapshots": items })))
}

async fn get_snapshot(
    State(service): State<Arc<ServerSnapshotService>>,
    AuthUser(_user, _): AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<SnapshotManifest>> {
    Ok(Json(service.get(id).await.map_err(map)?))
}

async fn preflight(
    State(service): State<Arc<ServerSnapshotService>>,
    AuthUser(user, _): AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    let (token, report) = service.preflight(&user, id).await.map_err(map)?;
    let report = serde_json::json!({
        "ready": report.ready,
        "warnings": report.warnings,
        "blockers": report.blockers,
        "collisions": report.collisions,
    });
    Ok(Json(
        serde_json::json!({ "confirm_token": token, "report": report }),
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RestoreInput {
    confirm_token: String,
    #[serde(default)]
    overwrite: bool,
}

async fn restore(
    State(service): State<Arc<ServerSnapshotService>>,
    AuthUser(user, _): AuthUser,
    Path(id): Path<Uuid>,
    Json(input): Json<RestoreInput>,
) -> ApiResult<Json<serde_json::Value>> {
    let applied = service
        .restore(&user, id, &input.confirm_token, input.overwrite)
        .await
        .map_err(map)?;
    Ok(Json(serde_json::json!({ "applied": applied })))
}

fn map(error: ServerSnapshotError) -> ApiError {
    match error {
        ServerSnapshotError::Forbidden => ApiError::Forbidden,
        ServerSnapshotError::NotFound(message) => ApiError::NotFound(message),
        ServerSnapshotError::Validation(message) => ApiError::Unprocessable(message),
        ServerSnapshotError::Confirmation(_) => ApiError::Unprocessable(error.to_string()),
        ServerSnapshotError::Partial { .. } => ApiError::Internal(error.to_string()),
        ServerSnapshotError::Failure(message) => ApiError::Internal(message),
    }
}
