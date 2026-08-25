//! Authenticated hosted-mail routes.
use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post},
};
use openpanel_app::{
    MailFilterService, MailingListService,
    mail::{
        DomainDeletionPreview, MailAlias, MailDomain, MailService, MailServiceError, MailStatus,
        Mailbox, MailboxCredential, Readiness,
    },
};
use openpanel_domain::{
    mail::MailQuota,
    mail_filtering::{
        AutoResponder, AutoResponderMode, CatchAll, Forwarder, MailingList, SieveScript,
    },
};
use serde::Deserialize;
use uuid::Uuid;

use crate::{ApiError, ApiResult, AuthUser};
/// Build `/mail` routes.
pub fn router(
    service: Arc<MailService>,
    filters: Arc<MailFilterService>,
    mailing_lists: Arc<MailingListService>,
) -> Router {
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
        .route("/queue", get(queue))
        .with_state(service)
        .merge(filter_routes(filters, mailing_lists))
}

// ---- mail-filtering surfaces (sieve / autoresponder / forwarders /
// catch-all / lists / queue). The bounded context's services enforce
// Owner|Admin RBAC; these handlers only map errors.

type FilterState = (Arc<MailFilterService>, Arc<MailingListService>);

fn filter_routes(
    filters: Arc<MailFilterService>,
    mailing_lists: Arc<MailingListService>,
) -> Router {
    Router::new()
        .route("/mailboxes/{id}/filters", get(get_filters).put(put_filters))
        .route(
            "/mailboxes/{id}/autoresponder",
            get(get_autoresponder)
                .put(put_autoresponder)
                .delete(delete_autoresponder),
        )
        .route(
            "/mailboxes/{id}/forwarders",
            get(list_forwarders).post(add_forwarder),
        )
        .route(
            "/mailboxes/{id}/forwarders/{destination}",
            axum::routing::delete(delete_forwarder),
        )
        .route("/catchall/{domain}", get(get_catchall).put(put_catchall))
        .route("/lists", get(list_lists).post(upsert_list))
        .route("/lists/{address}", get(get_list).delete(remove_list))
        .with_state((filters, mailing_lists))
}

async fn get_filters(
    State(state): State<FilterState>,
    AuthUser(user, _): AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    let script = state
        .0
        .get_sieve(&user, id)
        .await
        .map_err(map_mail_filter)?;
    Ok(Json(match script {
        Some(script) => serde_json::json!({"script": script.script}),
        None => serde_json::json!({"script": null}),
    }))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PutFiltersInput {
    script: String,
}

async fn put_filters(
    State(state): State<FilterState>,
    AuthUser(user, _): AuthUser,
    Path(id): Path<Uuid>,
    Json(input): Json<PutFiltersInput>,
) -> ApiResult<Json<serde_json::Value>> {
    let script = SieveScript::new(id, input.script).map_err(map_mail_filter)?;
    let saved = state
        .0
        .set_sieve(&user, script)
        .await
        .map_err(map_mail_filter)?;
    Ok(Json(serde_json::json!({
        "script": saved.script,
        "bytes": saved.script.len(),
    })))
}

async fn get_autoresponder(
    State(state): State<FilterState>,
    AuthUser(user, _): AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    let responder = state
        .0
        .get_autoresponder(&user, id)
        .await
        .map_err(map_mail_filter)?;
    Ok(Json(match responder {
        Some(responder) => serde_json::json!({
            "enabled": responder.enabled,
            "body": responder.body,
            "mode": responder.mode.as_str(),
            "window_start": responder.window_start.to_rfc3339(),
            "window_end": responder.window_end.to_rfc3339(),
        }),
        None => serde_json::json!(null),
    }))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PutAutoresponderInput {
    enabled: bool,
    body: String,
    mode: AutoResponderMode,
    window_start: chrono::DateTime<chrono::Utc>,
    window_end: chrono::DateTime<chrono::Utc>,
}

async fn put_autoresponder(
    State(state): State<FilterState>,
    AuthUser(user, _): AuthUser,
    Path(id): Path<Uuid>,
    Json(input): Json<PutAutoresponderInput>,
) -> ApiResult<Json<serde_json::Value>> {
    let responder = AutoResponder {
        mailbox_id: id,
        enabled: input.enabled,
        body: input.body,
        mode: input.mode,
        window_start: input.window_start,
        window_end: input.window_end,
    };
    let saved = state
        .0
        .set_autoresponder(&user, responder)
        .await
        .map_err(map_mail_filter)?;
    Ok(Json(serde_json::json!({
        "enabled": saved.enabled,
        "mode": saved.mode.as_str(),
    })))
}

async fn delete_autoresponder(
    State(state): State<FilterState>,
    AuthUser(user, _): AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    state
        .0
        .disable_autoresponder(&user, id)
        .await
        .map_err(map_mail_filter)?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AddForwarderInput {
    destination: String,
    keep_local: bool,
}

async fn add_forwarder(
    State(state): State<FilterState>,
    AuthUser(user, _): AuthUser,
    Path(id): Path<Uuid>,
    Query(source): Query<ForwarderSource>,
    Json(input): Json<AddForwarderInput>,
) -> ApiResult<Json<Forwarder>> {
    let forwarder = Forwarder {
        mailbox_id: id,
        destination: input.destination,
        keep_local: input.keep_local,
    };
    let saved = state
        .0
        .add_forwarder(&user, forwarder, &source.source)
        .await
        .map_err(map_mail_filter)?;
    Ok(Json(saved))
}

/// Source local-part query parameter for forwarder operations.
#[derive(Deserialize)]
pub struct ForwarderSource {
    /// Local source address used for loop detection.
    source: String,
}

async fn list_forwarders(
    State(state): State<FilterState>,
    AuthUser(user, _): AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Vec<Forwarder>>> {
    Ok(Json(
        state
            .0
            .list_forwarders(&user, id)
            .await
            .map_err(map_mail_filter)?,
    ))
}

async fn delete_forwarder(
    State(state): State<FilterState>,
    AuthUser(user, _): AuthUser,
    Path((id, destination)): Path<(Uuid, String)>,
    Query(source): Query<ForwarderSource>,
) -> ApiResult<StatusCode> {
    state
        .0
        .remove_forwarder_for_source(&user, id, &source.source, &destination)
        .await
        .map_err(map_mail_filter)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn get_catchall(
    State(state): State<FilterState>,
    AuthUser(user, _): AuthUser,
    Path(domain): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let catch_all = state
        .0
        .get_catch_all(&user, &domain)
        .await
        .map_err(map_mail_filter)?;
    Ok(Json(match catch_all {
        Some(catch_all) => serde_json::json!({
            "domain": catch_all.domain,
            "destination_mailbox": catch_all.destination_mailbox.to_string(),
        }),
        None => serde_json::json!(null),
    }))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PutCatchallInput {
    destination_mailbox: Uuid,
}

async fn put_catchall(
    State(state): State<FilterState>,
    AuthUser(user, _): AuthUser,
    Path(domain): Path<String>,
    Json(input): Json<PutCatchallInput>,
) -> ApiResult<Json<CatchAll>> {
    let catch_all = CatchAll {
        domain,
        destination_mailbox: input.destination_mailbox,
    };
    let saved = state
        .0
        .set_catch_all(&user, catch_all)
        .await
        .map_err(map_mail_filter)?;
    Ok(Json(saved))
}

async fn list_lists(
    State(state): State<FilterState>,
    AuthUser(user, _): AuthUser,
) -> ApiResult<Json<Vec<MailingList>>> {
    Ok(Json(
        state.1.list_all(&user).await.map_err(map_mail_filter)?,
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UpsertListInput {
    address: String,
    members: Vec<Uuid>,
}

async fn upsert_list(
    State(state): State<FilterState>,
    AuthUser(user, _): AuthUser,
    Json(input): Json<UpsertListInput>,
) -> ApiResult<Json<MailingList>> {
    let list = MailingList {
        address: input.address,
        members: input.members,
        created_at: chrono::Utc::now(),
    };
    let saved = state.1.upsert(&user, list).await.map_err(map_mail_filter)?;
    Ok(Json(saved))
}

async fn get_list(
    State(state): State<FilterState>,
    AuthUser(user, _): AuthUser,
    Path(address): Path<String>,
) -> ApiResult<Json<Option<MailingList>>> {
    Ok(Json(
        state
            .1
            .get_for_caller(&user, &address)
            .await
            .map_err(map_mail_filter)?,
    ))
}

async fn remove_list(
    State(state): State<FilterState>,
    AuthUser(user, _): AuthUser,
    Path(address): Path<String>,
) -> ApiResult<StatusCode> {
    state
        .1
        .remove(&user, &address)
        .await
        .map_err(map_mail_filter)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn queue(
    State(service): State<Arc<MailService>>,
    AuthUser(_user, _): AuthUser,
) -> ApiResult<Json<openpanel_domain::MailQueueSnapshot>> {
    Ok(Json(service.queue_snapshot().await.map_err(map)?))
}

fn map_mail_filter(error: openpanel_domain::MailFilterError) -> ApiError {
    use openpanel_domain::MailFilterError as Error;
    match error {
        Error::Forbidden => ApiError::Forbidden,
        Error::SieveTooLarge(_, _) | Error::SieveCompile(_) => {
            ApiError::Unprocessable("script_too_large".into())
        }
        Error::InvalidAutoResponder | Error::ForwarderLoop => {
            ApiError::Unprocessable(error.to_string())
        }
        Error::Persistence(message) => ApiError::Internal(message),
    }
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
