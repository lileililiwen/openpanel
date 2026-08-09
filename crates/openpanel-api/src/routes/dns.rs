//! Authenticated provider-backed DNS routes.

use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get, post},
};
use openpanel_app::dns::{
    DnsService, DnsServiceError, PropagationResult, ProviderAccount, ProviderZone, RemoteRecord,
    SyncResult,
};
use openpanel_domain::dns::RecordKind;
use serde::Deserialize;
use uuid::Uuid;

use crate::{ApiError, ApiResult, AuthUser};

/// Build `/dns` API routes.
pub fn router(service: Arc<DnsService>) -> Router {
    Router::new()
        .route("/providers", get(accounts).post(create_account))
        .route("/providers/{id}/sync", post(sync))
        .route("/providers/{id}/test", post(test_account))
        .route("/providers/{id}/rotate", post(rotate_account))
        .route("/providers/{id}/enable", post(enable_account))
        .route("/providers/{id}/disable", post(disable_account))
        .route("/providers/{id}", delete(delete_account))
        .route("/zones", get(zones))
        .route("/zones/{id}/records", get(records).post(create_record))
        .route(
            "/zones/{id}/records/{record_id}",
            delete(delete_record).put(update_record),
        )
        .route("/zones/{id}/check", post(check))
        .with_state(service)
}
#[derive(Deserialize)]
struct AccountInput {
    kind: String,
    name: String,
    credential: String,
}
async fn accounts(
    State(service): State<Arc<DnsService>>,
    _: AuthUser,
) -> ApiResult<Json<Vec<ProviderAccount>>> {
    Ok(Json(service.accounts().await.map_err(map)?))
}
async fn create_account(
    State(service): State<Arc<DnsService>>,
    AuthUser(user, _): AuthUser,
    Json(input): Json<AccountInput>,
) -> ApiResult<(StatusCode, Json<ProviderAccount>)> {
    let account = service
        .create_account(
            user.id(),
            user.role(),
            &input.kind,
            &input.name,
            &input.credential,
        )
        .await
        .map_err(map)?;
    Ok((StatusCode::CREATED, Json(account)))
}
async fn sync(
    State(service): State<Arc<DnsService>>,
    AuthUser(user, _): AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<SyncResult>> {
    Ok(Json(
        service
            .sync(user.id(), user.role(), id)
            .await
            .map_err(map)?,
    ))
}
async fn test_account(
    State(service): State<Arc<DnsService>>,
    AuthUser(user, _): AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<ProviderAccount>> {
    Ok(Json(
        service
            .test_account(user.id(), user.role(), id)
            .await
            .map_err(map)?,
    ))
}
#[derive(Deserialize)]
struct RotateInput {
    credential: String,
}
async fn rotate_account(
    State(service): State<Arc<DnsService>>,
    AuthUser(user, _): AuthUser,
    Path(id): Path<Uuid>,
    Json(input): Json<RotateInput>,
) -> ApiResult<Json<ProviderAccount>> {
    Ok(Json(
        service
            .rotate_account(user.id(), user.role(), id, &input.credential)
            .await
            .map_err(map)?,
    ))
}
async fn enable_account(
    State(service): State<Arc<DnsService>>,
    AuthUser(user, _): AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<ProviderAccount>> {
    Ok(Json(
        service
            .set_account_enabled(user.id(), user.role(), id, true)
            .await
            .map_err(map)?,
    ))
}
async fn disable_account(
    State(service): State<Arc<DnsService>>,
    AuthUser(user, _): AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<ProviderAccount>> {
    Ok(Json(
        service
            .set_account_enabled(user.id(), user.role(), id, false)
            .await
            .map_err(map)?,
    ))
}
async fn delete_account(
    State(service): State<Arc<DnsService>>,
    AuthUser(user, _): AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    service
        .delete_account(user.id(), user.role(), id)
        .await
        .map_err(map)?;
    Ok(StatusCode::NO_CONTENT)
}
async fn zones(
    State(service): State<Arc<DnsService>>,
    _: AuthUser,
) -> ApiResult<Json<Vec<ProviderZone>>> {
    Ok(Json(service.zones().await.map_err(map)?))
}
async fn records(
    State(service): State<Arc<DnsService>>,
    _: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Vec<RemoteRecord>>> {
    Ok(Json(service.records(id).await.map_err(map)?))
}
#[derive(Deserialize)]
struct RecordInput {
    name: String,
    kind: RecordKind,
    value: String,
    ttl: u32,
    expected_version: String,
}
async fn create_record(
    State(service): State<Arc<DnsService>>,
    AuthUser(user, _): AuthUser,
    Path(id): Path<Uuid>,
    Json(input): Json<RecordInput>,
) -> ApiResult<(StatusCode, Json<RemoteRecord>)> {
    let record = service
        .create_record(
            user.id(),
            user.role(),
            id,
            &input.name,
            input.kind,
            &input.value,
            input.ttl,
            &input.expected_version,
        )
        .await
        .map_err(map)?;
    Ok((StatusCode::CREATED, Json(record)))
}
async fn update_record(
    State(service): State<Arc<DnsService>>,
    AuthUser(user, _): AuthUser,
    Path((id, record_id)): Path<(Uuid, String)>,
    Json(input): Json<RecordInput>,
) -> ApiResult<Json<RemoteRecord>> {
    Ok(Json(
        service
            .update_record(
                user.id(),
                user.role(),
                id,
                &record_id,
                &input.name,
                input.kind,
                &input.value,
                input.ttl,
                &input.expected_version,
            )
            .await
            .map_err(map)?,
    ))
}
#[derive(Deserialize)]
struct DeleteQuery {
    expected_version: String,
}
async fn delete_record(
    State(service): State<Arc<DnsService>>,
    AuthUser(user, _): AuthUser,
    Path((id, record_id)): Path<(Uuid, String)>,
    axum::extract::Query(query): axum::extract::Query<DeleteQuery>,
) -> ApiResult<StatusCode> {
    service
        .delete_record(
            user.id(),
            user.role(),
            id,
            &record_id,
            &query.expected_version,
        )
        .await
        .map_err(map)?;
    Ok(StatusCode::NO_CONTENT)
}
async fn check(
    State(service): State<Arc<DnsService>>,
    _: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<PropagationResult>> {
    Ok(Json(service.check(id).await.map_err(map)?))
}
fn map(error: DnsServiceError) -> ApiError {
    match error {
        DnsServiceError::Forbidden => ApiError::Forbidden,
        DnsServiceError::NotFound => ApiError::NotFound("dns object".into()),
        DnsServiceError::Conflict => ApiError::Conflict(error.to_string()),
        DnsServiceError::Invalid => ApiError::Unprocessable(error.to_string()),
        DnsServiceError::Provider(_) | DnsServiceError::Repository | DnsServiceError::Crypto => {
            ApiError::Internal(error.to_string())
        }
    }
}
