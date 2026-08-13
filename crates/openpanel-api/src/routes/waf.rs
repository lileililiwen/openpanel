//! Owner-only per-site WAF routes.

use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{get, post},
};
use openpanel_app::WafService;
use openpanel_domain::waf::{DefaultAction, DryRunRequest, Rule, RuleSet, WafError, WafHit};
use serde::Deserialize;
use uuid::Uuid;

use crate::{ApiError, ApiResult, AuthUser};

/// Build routes nested under `/sites`.
pub fn router(service: Arc<WafService>) -> Router {
    Router::new()
        .route("/{id}/waf", get(get_rules).put(put_rules))
        .route("/{id}/waf/hits", get(hits))
        .route("/{id}/waf/test", post(dry_run))
        .with_state(service)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PutInput {
    version: u64,
    default_action: DefaultAction,
    rules: Vec<Rule>,
}

async fn get_rules(
    State(service): State<Arc<WafService>>,
    AuthUser(user, _): AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<RuleSet>> {
    Ok(Json(service.get(&user, id).await.map_err(map)?))
}

async fn put_rules(
    State(service): State<Arc<WafService>>,
    AuthUser(user, _): AuthUser,
    Path(id): Path<Uuid>,
    Json(input): Json<PutInput>,
) -> ApiResult<Json<RuleSet>> {
    let set = RuleSet::new(id, input.version, input.default_action, input.rules).map_err(map)?;
    Ok(Json(service.put(&user, set).await.map_err(map)?))
}

async fn hits(
    State(service): State<Arc<WafService>>,
    AuthUser(user, _): AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Vec<WafHit>>> {
    Ok(Json(service.hits(&user, id).await.map_err(map)?))
}

async fn dry_run(
    State(service): State<Arc<WafService>>,
    AuthUser(user, _): AuthUser,
    Path(id): Path<Uuid>,
    Json(input): Json<DryRunRequest>,
) -> ApiResult<Json<openpanel_app::waf::DryRunResult>> {
    Ok(Json(service.dry_run(&user, id, input).await.map_err(map)?))
}

fn map(error: WafError) -> ApiError {
    match error {
        WafError::Forbidden => ApiError::Forbidden,
        WafError::SiteNotFound(message) => ApiError::NotFound(message),
        WafError::Invalid(message) => ApiError::Unprocessable(message),
        WafError::Compile(message) | WafError::Persistence(message) => ApiError::Internal(message),
    }
}
