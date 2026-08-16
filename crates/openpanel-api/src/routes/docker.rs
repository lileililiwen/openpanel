//! Owner-only container lifecycle API.

use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    routing::{delete, get, post},
};
use openpanel_app::{DockerService, ExecResult};
use openpanel_domain::docker::{
    ComposeStack, Container, ContainerSpec, DockerError, ImageAllowlistEntry, StoredComposeStack,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::{ApiError, ApiResult, AuthUser};

/// Build routes nested under `/docker`.
pub fn router(service: Arc<DockerService>) -> Router {
    Router::new()
        .route("/images/allowlist", get(allowlist).post(put_allowlist))
        .route("/images/pull", post(pull))
        .route("/containers", get(list).post(create))
        .route("/containers/{id}", get(inspect).delete(remove))
        .route("/containers/{id}/{action}", post(action))
        .route("/containers/{id}/logs", get(logs))
        .route("/containers/{id}/exec", post(exec))
        .route("/stacks", get(stacks).post(put_stack))
        .route("/stacks/{id}/apply", post(apply_stack))
        .route("/stacks/{id}", delete(delete_stack))
        .with_state(service)
}

async fn allowlist(
    State(service): State<Arc<DockerService>>,
    AuthUser(user, _): AuthUser,
) -> ApiResult<Json<Vec<ImageAllowlistEntry>>> {
    Ok(Json(service.allowlist(&user).await.map_err(map)?))
}
async fn put_allowlist(
    State(service): State<Arc<DockerService>>,
    AuthUser(user, _): AuthUser,
    Json(entry): Json<ImageAllowlistEntry>,
) -> ApiResult<Json<serde_json::Value>> {
    service.put_allowlist(&user, entry).await.map_err(map)?;
    Ok(Json(serde_json::json!({"ok":true})))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PullInput {
    image: String,
}
async fn pull(
    State(service): State<Arc<DockerService>>,
    AuthUser(user, _): AuthUser,
    Json(input): Json<PullInput>,
) -> ApiResult<Json<serde_json::Value>> {
    let digest = service.pull(&user, &input.image).await.map_err(map)?;
    Ok(Json(serde_json::json!({"digest":digest})))
}
async fn list(
    State(service): State<Arc<DockerService>>,
    AuthUser(user, _): AuthUser,
) -> ApiResult<Json<Vec<Container>>> {
    Ok(Json(service.list(&user).await.map_err(map)?))
}
async fn create(
    State(service): State<Arc<DockerService>>,
    AuthUser(user, _): AuthUser,
    Json(spec): Json<ContainerSpec>,
) -> ApiResult<Json<Container>> {
    Ok(Json(service.create(&user, spec).await.map_err(map)?))
}
async fn inspect(
    State(service): State<Arc<DockerService>>,
    AuthUser(user, _): AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Container>> {
    Ok(Json(service.inspect(&user, id).await.map_err(map)?))
}
async fn action(
    State(service): State<Arc<DockerService>>,
    AuthUser(user, _): AuthUser,
    Path((id, action)): Path<(Uuid, String)>,
) -> ApiResult<Json<Container>> {
    Ok(Json(service.action(&user, id, &action).await.map_err(map)?))
}
#[derive(Deserialize)]
struct LogQuery {
    tail: Option<u64>,
}
async fn logs(
    State(service): State<Arc<DockerService>>,
    AuthUser(user, _): AuthUser,
    Path(id): Path<Uuid>,
    Query(query): Query<LogQuery>,
) -> ApiResult<Json<Vec<String>>> {
    Ok(Json(
        service
            .logs(&user, id, query.tail.unwrap_or(100))
            .await
            .map_err(map)?,
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExecInput {
    command: Vec<String>,
}
async fn exec(
    State(service): State<Arc<DockerService>>,
    AuthUser(user, _): AuthUser,
    Path(id): Path<Uuid>,
    Json(input): Json<ExecInput>,
) -> ApiResult<Json<ExecResult>> {
    Ok(Json(
        service.exec(&user, id, input.command).await.map_err(map)?,
    ))
}
#[derive(Deserialize)]
struct RemoveQuery {
    force: Option<bool>,
}
async fn remove(
    State(service): State<Arc<DockerService>>,
    AuthUser(user, _): AuthUser,
    Path(id): Path<Uuid>,
    Query(query): Query<RemoveQuery>,
) -> ApiResult<Json<serde_json::Value>> {
    service
        .remove(&user, id, query.force.unwrap_or(false))
        .await
        .map_err(map)?;
    Ok(Json(serde_json::json!({"ok":true})))
}
async fn stacks(
    State(service): State<Arc<DockerService>>,
    AuthUser(user, _): AuthUser,
) -> ApiResult<Json<Vec<StoredComposeStack>>> {
    Ok(Json(service.stacks(&user).await.map_err(map)?))
}
async fn put_stack(
    State(service): State<Arc<DockerService>>,
    AuthUser(user, _): AuthUser,
    Json(stack): Json<ComposeStack>,
) -> ApiResult<Json<StoredComposeStack>> {
    Ok(Json(service.put_stack(&user, stack).await.map_err(map)?))
}
async fn apply_stack(
    State(service): State<Arc<DockerService>>,
    AuthUser(user, _): AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<openpanel_app::ApplyReport>> {
    Ok(Json(service.apply_stack(&user, id).await.map_err(map)?))
}
async fn delete_stack(
    State(service): State<Arc<DockerService>>,
    AuthUser(user, _): AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    service.delete_stack(&user, id).await.map_err(map)?;
    Ok(Json(serde_json::json!({"ok":true})))
}
fn map(error: DockerError) -> ApiError {
    match error {
        DockerError::Forbidden | DockerError::ImageDenied(_) => ApiError::Forbidden,
        DockerError::Invalid(message) => ApiError::Unprocessable(message),
        DockerError::NotFound(message) => ApiError::NotFound(message),
        DockerError::QuotaExceeded => ApiError::Unprocessable("container quota exceeded".into()),
        DockerError::Adapter(message) | DockerError::Persistence(message) => {
            ApiError::Internal(message)
        }
    }
}
