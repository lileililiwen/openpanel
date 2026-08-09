//! Owner-only host firewall and login-abuse administration routes.

use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, post},
};
use openpanel_app::{SecurityService, security::SecurityServiceError};
use openpanel_domain::{
    Role,
    security::{FirewallRule, LoginKey, NetworkCidr, PortRange, Protocol, RuleAction},
};
use serde::Deserialize;
use uuid::Uuid;

use crate::{ApiError, ApiResult, AuthUser};

/// Build `/security` routes.
pub fn router(service: Arc<SecurityService>) -> Router {
    Router::new()
        .route("/status", get(status))
        .route("/posture", get(posture))
        .route("/rules", get(rules).post(create))
        .route("/rules/{id}", post(update).put(update).delete(remove))
        .route("/rules/{id}/enable", post(enable))
        .route("/rules/{id}/disable", post(disable))
        .route("/preview", post(preview))
        .route("/apply", post(apply))
        .route("/rollback", post(rollback))
        .route("/blocks", get(blocks))
        .route("/blocks/{key}", delete(unblock))
        .route("/allowlists", get(allowlists).post(add_allowlist))
        .route("/allowlists/{network}", delete(remove_allowlist))
        .with_state(service)
}

#[derive(Deserialize)]
struct RuleInput {
    protocol: Protocol,
    port_start: u16,
    port_end: u16,
    source: String,
    action: RuleAction,
    comment: String,
    enabled: bool,
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct RuleUpdate {
    protocol: Option<Protocol>,
    port_start: Option<u16>,
    port_end: Option<u16>,
    source: Option<String>,
    action: Option<RuleAction>,
    comment: Option<String>,
    enabled: Option<bool>,
}

#[derive(Deserialize)]
struct AllowlistInput {
    network: String,
}

fn require_owner(user: &openpanel_domain::User) -> ApiResult<()> {
    if matches!(user.role(), Role::Owner) {
        Ok(())
    } else {
        Err(ApiError::Forbidden)
    }
}

fn build_rule(id: Uuid, input: RuleInput) -> ApiResult<FirewallRule> {
    FirewallRule::new(
        id,
        input.protocol,
        PortRange::new(input.port_start, input.port_end).map_err(validation)?,
        NetworkCidr::parse(&input.source).map_err(validation)?,
        input.action,
        input.comment,
        input.enabled,
    )
    .map_err(validation)
}

async fn status(
    State(service): State<Arc<SecurityService>>,
    AuthUser(user, _): AuthUser,
) -> ApiResult<Json<serde_json::Value>> {
    require_owner(&user)?;
    Ok(Json(
        serde_json::json!({"supported": service.supported().await.map_err(map)?}),
    ))
}

async fn posture(AuthUser(user, _): AuthUser) -> ApiResult<Json<serde_json::Value>> {
    require_owner(&user)?;
    Ok(Json(
        serde_json::json!({"managed_table":"inet openpanel","protected_ports":[22,8443],"login_abuse_protection":true}),
    ))
}

async fn rules(
    State(service): State<Arc<SecurityService>>,
    AuthUser(user, _): AuthUser,
) -> ApiResult<Json<Vec<FirewallRule>>> {
    require_owner(&user)?;
    Ok(Json(service.rules().await.map_err(map)?))
}

async fn create(
    State(service): State<Arc<SecurityService>>,
    AuthUser(user, _): AuthUser,
    Json(input): Json<RuleInput>,
) -> ApiResult<impl IntoResponse> {
    require_owner(&user)?;
    Ok((
        StatusCode::CREATED,
        Json(
            service
                .save_rule(build_rule(Uuid::new_v4(), input)?)
                .await
                .map_err(map)?,
        ),
    ))
}

async fn update(
    State(service): State<Arc<SecurityService>>,
    AuthUser(user, _): AuthUser,
    Path(id): Path<Uuid>,
    Json(input): Json<RuleUpdate>,
) -> ApiResult<Json<FirewallRule>> {
    require_owner(&user)?;
    let old = service.rule(id).await.map_err(map)?;
    let ports = old.ports();
    let rule = FirewallRule::new(
        id,
        input.protocol.unwrap_or(old.protocol()),
        PortRange::new(
            input.port_start.unwrap_or(ports.start()),
            input.port_end.unwrap_or(ports.end()),
        )
        .map_err(validation)?,
        match input.source {
            Some(source) => NetworkCidr::parse(&source).map_err(validation)?,
            None => old.source(),
        },
        input.action.unwrap_or(old.action()),
        input.comment.unwrap_or_else(|| old.comment().to_string()),
        input.enabled.unwrap_or(old.enabled()),
    )
    .map_err(validation)?;
    Ok(Json(service.save_rule(rule).await.map_err(map)?))
}

async fn enable(
    State(service): State<Arc<SecurityService>>,
    AuthUser(user, _): AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<FirewallRule>> {
    require_owner(&user)?;
    Ok(Json(service.set_rule_enabled(id, true).await.map_err(map)?))
}
async fn disable(
    State(service): State<Arc<SecurityService>>,
    AuthUser(user, _): AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<FirewallRule>> {
    require_owner(&user)?;
    Ok(Json(
        service.set_rule_enabled(id, false).await.map_err(map)?,
    ))
}
async fn remove(
    State(service): State<Arc<SecurityService>>,
    AuthUser(user, _): AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    require_owner(&user)?;
    service.delete_rule(id).await.map_err(map)?;
    Ok(StatusCode::NO_CONTENT)
}
async fn preview(
    State(service): State<Arc<SecurityService>>,
    AuthUser(user, _): AuthUser,
) -> ApiResult<String> {
    require_owner(&user)?;
    service.preview_saved(None).await.map_err(map)
}
async fn apply(
    State(service): State<Arc<SecurityService>>,
    AuthUser(user, _): AuthUser,
) -> ApiResult<StatusCode> {
    require_owner(&user)?;
    service.apply_saved(user.id(), None).await.map_err(map)?;
    Ok(StatusCode::OK)
}
async fn rollback(
    State(service): State<Arc<SecurityService>>,
    AuthUser(user, _): AuthUser,
) -> ApiResult<StatusCode> {
    require_owner(&user)?;
    service.rollback(user.id()).await.map_err(map)?;
    Ok(StatusCode::OK)
}
async fn blocks(
    State(service): State<Arc<SecurityService>>,
    AuthUser(user, _): AuthUser,
) -> ApiResult<Json<Vec<openpanel_domain::security::TemporaryBlock>>> {
    require_owner(&user)?;
    Ok(Json(service.blocks().await.map_err(map)?))
}
async fn unblock(
    State(service): State<Arc<SecurityService>>,
    AuthUser(user, _): AuthUser,
    Path(key): Path<String>,
) -> ApiResult<StatusCode> {
    require_owner(&user)?;
    service
        .unblock(user.id(), LoginKey::stored(&key).map_err(validation)?)
        .await
        .map_err(map)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn allowlists(
    State(service): State<Arc<SecurityService>>,
    AuthUser(user, _): AuthUser,
) -> ApiResult<Json<Vec<NetworkCidr>>> {
    require_owner(&user)?;
    Ok(Json(service.allowlists().await.map_err(map)?))
}

async fn add_allowlist(
    State(service): State<Arc<SecurityService>>,
    AuthUser(user, _): AuthUser,
    Json(input): Json<AllowlistInput>,
) -> ApiResult<impl IntoResponse> {
    require_owner(&user)?;
    let network = NetworkCidr::parse(&input.network).map_err(validation)?;
    Ok((
        StatusCode::CREATED,
        Json(
            service
                .add_allowlist(user.id(), network)
                .await
                .map_err(map)?,
        ),
    ))
}

async fn remove_allowlist(
    State(service): State<Arc<SecurityService>>,
    AuthUser(user, _): AuthUser,
    Path(network): Path<String>,
) -> ApiResult<StatusCode> {
    require_owner(&user)?;
    service
        .remove_allowlist(user.id(), NetworkCidr::parse(&network).map_err(validation)?)
        .await
        .map_err(map)?;
    Ok(StatusCode::NO_CONTENT)
}

fn validation(error: openpanel_domain::security::SecurityError) -> ApiError {
    ApiError::Unprocessable(error.to_string())
}
fn map(error: SecurityServiceError) -> ApiError {
    match error {
        SecurityServiceError::NotFound => ApiError::NotFound("security record".into()),
        SecurityServiceError::Validation(message) => ApiError::Unprocessable(message),
        SecurityServiceError::SyntaxRejected | SecurityServiceError::VerificationFailed => {
            ApiError::Conflict(error.to_string())
        }
        SecurityServiceError::Firewall | SecurityServiceError::Persistence => {
            ApiError::Internal(error.to_string())
        }
    }
}
