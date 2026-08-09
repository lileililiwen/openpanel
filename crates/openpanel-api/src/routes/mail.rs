//! Authenticated hosted-mail routes.
use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
};
use openpanel_app::mail::{
    DomainDeletionPreview, MailAlias, MailDomain, MailService, MailServiceError, MailStatus,
    Mailbox, MailboxCredential, Readiness,
};
use openpanel_domain::mail::MailQuota;
use serde::Deserialize;
use uuid::Uuid;

use crate::{ApiError, ApiResult, AuthUser};
/// Build `/mail` routes.
pub fn router(service: Arc<MailService>) -> Router {
    Router::new()
        .route("/readiness", get(readiness))
        .route("/domains", get(domains).post(create_domain))
        .route("/domains/{id}/enable", post(enable_domain))
        .route("/domains/{id}/disable", post(disable_domain))
        .route("/domains/{id}/delete-preview", post(delete_preview))
        .route("/domains/{id}/delete", post(delete_domain))
        .route(
            "/domains/{id}/mailboxes",
            get(mailboxes).post(create_mailbox),
        )
        .route("/domains/{id}/aliases", get(aliases).post(create_alias))
        .route("/mailboxes/quota", post(quota))
        .route("/mailboxes/password", post(password))
        .route("/mailboxes/enabled", post(mailbox_enabled))
        .route("/mailboxes/delete", post(delete_mailbox))
        .route("/aliases/{id}/delete", post(delete_alias))
        .route("/status", get(status))
        .with_state(service)
}
async fn readiness(
    State(service): State<Arc<MailService>>,
    _: AuthUser,
) -> ApiResult<Json<Readiness>> {
    Ok(Json(service.readiness().await.map_err(map)?))
}
#[derive(Deserialize)]
struct DomainInput {
    name: String,
}
async fn create_domain(
    State(service): State<Arc<MailService>>,
    AuthUser(user, _): AuthUser,
    Json(input): Json<DomainInput>,
) -> ApiResult<(StatusCode, Json<MailDomain>)> {
    Ok((
        StatusCode::CREATED,
        Json(
            service
                .create_domain(user.id(), user.role(), &input.name)
                .await
                .map_err(map)?,
        ),
    ))
}
async fn domains(
    State(service): State<Arc<MailService>>,
    AuthUser(user, _): AuthUser,
) -> ApiResult<Json<Vec<MailDomain>>> {
    Ok(Json(
        service.domains(user.id(), user.role()).await.map_err(map)?,
    ))
}
#[derive(Deserialize)]
struct EnableInput {
    #[serde(default)]
    acknowledged: bool,
}
async fn enable_domain(
    State(service): State<Arc<MailService>>,
    AuthUser(user, _): AuthUser,
    Path(id): Path<Uuid>,
    Json(input): Json<EnableInput>,
) -> ApiResult<Json<MailDomain>> {
    Ok(Json(
        service
            .enable_domain(user.id(), user.role(), id, input.acknowledged)
            .await
            .map_err(map)?,
    ))
}
async fn disable_domain(
    State(service): State<Arc<MailService>>,
    AuthUser(user, _): AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<MailDomain>> {
    Ok(Json(
        service
            .disable_domain(user.id(), user.role(), id)
            .await
            .map_err(map)?,
    ))
}
async fn delete_preview(
    State(service): State<Arc<MailService>>,
    AuthUser(user, _): AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<DomainDeletionPreview>> {
    Ok(Json(
        service
            .preview_delete_domain(user.id(), user.role(), id)
            .await
            .map_err(map)?,
    ))
}
#[derive(Deserialize)]
struct DeleteInput {
    confirmation_token: String,
}
async fn delete_domain(
    State(service): State<Arc<MailService>>,
    AuthUser(user, _): AuthUser,
    Path(id): Path<Uuid>,
    Json(input): Json<DeleteInput>,
) -> ApiResult<StatusCode> {
    service
        .delete_domain(user.id(), user.role(), id, &input.confirmation_token)
        .await
        .map_err(map)?;
    Ok(StatusCode::NO_CONTENT)
}
#[derive(Deserialize)]
struct MailboxInput {
    local: String,
    quota_bytes: u64,
    password: Option<String>,
}
async fn create_mailbox(
    State(service): State<Arc<MailService>>,
    AuthUser(user, _): AuthUser,
    Path(id): Path<Uuid>,
    Json(input): Json<MailboxInput>,
) -> ApiResult<(StatusCode, Json<MailboxCredential>)> {
    let quota = MailQuota::new(input.quota_bytes, 1024, 1_073_741_824)
        .map_err(|_| ApiError::Unprocessable("invalid quota".into()))?;
    Ok((
        StatusCode::CREATED,
        Json(
            service
                .create_mailbox(
                    user.id(),
                    user.role(),
                    id,
                    &input.local,
                    quota,
                    input.password.as_deref(),
                )
                .await
                .map_err(map)?,
        ),
    ))
}
async fn mailboxes(
    State(service): State<Arc<MailService>>,
    AuthUser(user, _): AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Vec<Mailbox>>> {
    Ok(Json(
        service
            .mailboxes(user.id(), user.role(), id)
            .await
            .map_err(map)?,
    ))
}
#[derive(Deserialize)]
struct AliasInput {
    source: String,
    destination: String,
}
async fn create_alias(
    State(service): State<Arc<MailService>>,
    AuthUser(user, _): AuthUser,
    Path(id): Path<Uuid>,
    Json(input): Json<AliasInput>,
) -> ApiResult<(StatusCode, Json<MailAlias>)> {
    Ok((
        StatusCode::CREATED,
        Json(
            service
                .add_alias(
                    user.id(),
                    user.role(),
                    id,
                    &input.source,
                    &input.destination,
                )
                .await
                .map_err(map)?,
        ),
    ))
}
async fn aliases(
    State(service): State<Arc<MailService>>,
    AuthUser(user, _): AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Vec<MailAlias>>> {
    Ok(Json(
        service
            .aliases(user.id(), user.role(), id)
            .await
            .map_err(map)?,
    ))
}
#[derive(Deserialize)]
struct QuotaInput {
    address: String,
    quota_bytes: u64,
}
async fn quota(
    State(service): State<Arc<MailService>>,
    AuthUser(user, _): AuthUser,
    Json(input): Json<QuotaInput>,
) -> ApiResult<Json<Mailbox>> {
    let quota = MailQuota::new(input.quota_bytes, 1024, 1_073_741_824)
        .map_err(|_| ApiError::Unprocessable("invalid quota".into()))?;
    Ok(Json(
        service
            .update_quota(user.id(), user.role(), &input.address, quota)
            .await
            .map_err(map)?,
    ))
}
#[derive(Deserialize)]
struct PasswordInput {
    address: String,
    password: String,
}
async fn password(
    State(service): State<Arc<MailService>>,
    AuthUser(user, _): AuthUser,
    Json(input): Json<PasswordInput>,
) -> ApiResult<Json<MailboxCredential>> {
    Ok(Json(
        service
            .rotate_password(user.id(), user.role(), &input.address, &input.password)
            .await
            .map_err(map)?,
    ))
}
#[derive(Deserialize)]
struct MailboxEnabledInput {
    address: String,
    enabled: bool,
}
async fn mailbox_enabled(
    State(service): State<Arc<MailService>>,
    AuthUser(user, _): AuthUser,
    Json(input): Json<MailboxEnabledInput>,
) -> ApiResult<Json<Mailbox>> {
    Ok(Json(
        service
            .set_mailbox_enabled(user.id(), user.role(), &input.address, input.enabled)
            .await
            .map_err(map)?,
    ))
}
#[derive(Deserialize)]
struct ConfirmedAddressInput {
    address: String,
    #[serde(default)]
    confirmed: bool,
}
async fn delete_mailbox(
    State(service): State<Arc<MailService>>,
    AuthUser(user, _): AuthUser,
    Json(input): Json<ConfirmedAddressInput>,
) -> ApiResult<StatusCode> {
    service
        .delete_mailbox(user.id(), user.role(), &input.address, input.confirmed)
        .await
        .map_err(map)?;
    Ok(StatusCode::NO_CONTENT)
}
#[derive(Deserialize)]
struct ConfirmInput {
    #[serde(default)]
    confirmed: bool,
}
async fn delete_alias(
    State(service): State<Arc<MailService>>,
    AuthUser(user, _): AuthUser,
    Path(id): Path<Uuid>,
    Json(input): Json<ConfirmInput>,
) -> ApiResult<StatusCode> {
    service
        .delete_alias(user.id(), user.role(), id, input.confirmed)
        .await
        .map_err(map)?;
    Ok(StatusCode::NO_CONTENT)
}
async fn status(
    State(service): State<Arc<MailService>>,
    _: AuthUser,
) -> ApiResult<Json<MailStatus>> {
    Ok(Json(service.status().await.map_err(map)?))
}
fn map(error: MailServiceError) -> ApiError {
    match error {
        MailServiceError::Forbidden => ApiError::Forbidden,
        MailServiceError::NotFound => ApiError::NotFound("mail object".into()),
        MailServiceError::NotReady(_) => ApiError::Conflict(error.to_string()),
        MailServiceError::Invalid => ApiError::Unprocessable(error.to_string()),
        MailServiceError::Configuration | MailServiceError::Repository => {
            ApiError::Internal(error.to_string())
        }
    }
}
