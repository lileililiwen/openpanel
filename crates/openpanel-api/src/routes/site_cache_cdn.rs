//! Site cache and CDN integration HTTP routes.
//!
//! Mounted under `/api/v1`:
//!
//! * `GET    /sites/{id}/cache`        — read the site's `SiteCachePolicy`
//! * `PUT    /sites/{id}/cache`        — replace the policy
//! * `POST   /sites/{id}/cache/purge`  — purge paths from the local cache
//! * `GET    /cdn/integrations`        — list CDN integrations (metadata only)
//! * `POST   /cdn/integrations`        — create an integration
//! * `DELETE /cdn/integrations/{id}`   — delete an integration
//! * `POST   /cdn/purge`               — purge through an integration
//!
//! Responses NEVER include the stored `config_enc` blob nor any
//! provider secret. Only `id`, `name`, `kind`, `zone_id`, `enabled`,
//! and `created_at` are returned.

use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};
use openpanel_app::site_cache_cdn::SiteCacheService;
use openpanel_domain::{CdnKind, SiteCachePolicy};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{ApiError, ApiResult, AuthUser};

/// Build the cache+CDN routes (merged into the top-level `/api/v1` router).
///
/// The returned router exposes the canonical endpoints from the
/// `site-cache-cdn` design doc under two prefixes:
///
/// * `/sites/{id}/cache`  — `GET` (read policy) / `PUT` (replace) / `POST .../cache/purge`
/// * `/cdn/integrations`  — `GET` (list) / `POST` (create) / `DELETE /cdn/integrations/{id}`
/// * `/cdn/purge`         — `POST` (purge through an integration)
///
/// Responses NEVER include the stored `config_enc` blob nor any
/// provider secret. Only `id`, `name`, `kind`, `enabled`, and
/// `created_at` are returned.
pub fn router(service: Arc<SiteCacheService>) -> Router {
    let cache = Router::new()
        .route("/sites/{id}/cache", get(get_cache).put(put_cache))
        .route("/sites/{id}/cache/purge", post(purge_site_cache))
        .with_state(service.clone());
    let cdn = Router::new()
        .route(
            "/cdn/integrations",
            get(list_integrations).post(create_integration),
        )
        .route(
            "/cdn/integrations/{id}",
            axum::routing::delete(delete_integration),
        )
        .route("/cdn/purge", post(purge))
        .with_state(service);
    cache.merge(cdn)
}

// ---- DTOs ---------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PutCacheBody {
    ttl_seconds: u32,
    #[serde(default)]
    static_assets_ttl_seconds: Option<u32>,
    #[serde(default)]
    bypass_paths: Vec<String>,
    #[serde(default)]
    keyed_cookies: Vec<String>,
    #[serde(default)]
    stale_while_revalidate: bool,
    #[serde(default = "default_revalidation")]
    revalidation_required: bool,
}

fn default_revalidation() -> bool {
    true
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PurgePathsBody {
    paths: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CdnPurgeBody {
    integration_id: Uuid,
    paths: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateIntegrationBody {
    name: String,
    kind: CdnKind,
    /// Provider-specific fields. Sent in plaintext; the panel
    /// stores them as an opaque JSON blob (`config_enc`). They are
    /// never echoed back in any response.
    #[serde(default)]
    api_token: Option<String>,
    #[serde(default)]
    zone_id: Option<String>,
    #[serde(default)]
    webhook_url: Option<String>,
    #[serde(default)]
    region: Option<String>,
    #[serde(default)]
    secret_key: Option<String>,
}

#[derive(Debug, Serialize)]
struct IntegrationView {
    id: Uuid,
    name: String,
    kind: String,
    enabled: bool,
    created_at: chrono::DateTime<chrono::Utc>,
}

impl From<openpanel_domain::CdnIntegration> for IntegrationView {
    fn from(i: openpanel_domain::CdnIntegration) -> Self {
        Self {
            id: i.id(),
            name: i.name().to_string(),
            kind: i.kind().as_str().to_string(),
            enabled: i.enabled(),
            created_at: i.created_at(),
        }
    }
}

// ---- handlers ------------------------------------------------------------

async fn get_cache(
    State(svc): State<Arc<SiteCacheService>>,
    AuthUser(user, _): AuthUser,
    Path(site_id): Path<Uuid>,
) -> ApiResult<Json<CachePolicyView>> {
    require_owner(&user)?;
    let policy = svc.cache_policy(site_id).await.map_err(map)?;
    Ok(Json(CachePolicyView::from(policy)))
}

async fn put_cache(
    State(svc): State<Arc<SiteCacheService>>,
    AuthUser(user, _): AuthUser,
    Path(site_id): Path<Uuid>,
    Json(body): Json<PutCacheBody>,
) -> ApiResult<Json<CachePolicyView>> {
    require_owner(&user)?;
    let mut policy = SiteCachePolicy::with_ttl(site_id, body.ttl_seconds).map_err(map)?;
    if let Some(s) = body.static_assets_ttl_seconds {
        policy = policy.with_static_assets_ttl(s).map_err(map)?;
    }
    policy = policy.with_bypass_paths(body.bypass_paths).map_err(map)?;
    policy = policy.with_keyed_cookies(body.keyed_cookies).map_err(map)?;
    policy = policy
        .with_stale_while_revalidate(body.stale_while_revalidate)
        .with_revalidation_required(body.revalidation_required);
    svc.set_cache_policy(user.username().as_str(), &policy)
        .await
        .map_err(map)?;
    Ok(Json(CachePolicyView::from(policy)))
}

async fn purge_site_cache(
    State(svc): State<Arc<SiteCacheService>>,
    AuthUser(user, _): AuthUser,
    Path(site_id): Path<Uuid>,
    Json(body): Json<PurgePathsBody>,
) -> ApiResult<Json<openpanel_app::site_cache_cdn::PurgeSummary>> {
    require_owner(&user)?;
    let receipt = svc
        .purge_site_cache(user.username().as_str(), site_id, body.paths)
        .await
        .map_err(map)?;
    Ok(Json(openpanel_app::site_cache_cdn::PurgeSummary::new(
        receipt.purged().len(),
        0,
        Vec::new(),
    )))
}

async fn list_integrations(
    State(svc): State<Arc<SiteCacheService>>,
    AuthUser(user, _): AuthUser,
) -> ApiResult<Json<Vec<IntegrationView>>> {
    require_owner(&user)?;
    let rows = svc.list_integrations().await.map_err(map)?;
    Ok(Json(rows.into_iter().map(IntegrationView::from).collect()))
}

async fn create_integration(
    State(svc): State<Arc<SiteCacheService>>,
    AuthUser(user, _): AuthUser,
    Json(body): Json<CreateIntegrationBody>,
) -> ApiResult<impl IntoResponse> {
    require_owner(&user)?;
    let integration = svc
        .create_integration_for_provider(
            user.username().as_str(),
            &body.name,
            body.kind,
            body.api_token,
            body.zone_id,
            body.webhook_url,
            body.region,
            body.secret_key,
        )
        .await
        .map_err(map)?;
    Ok((
        StatusCode::CREATED,
        Json(IntegrationView::from(integration)),
    ))
}

async fn delete_integration(
    State(svc): State<Arc<SiteCacheService>>,
    AuthUser(user, _): AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    require_owner(&user)?;
    svc.delete_integration(user.username().as_str(), id)
        .await
        .map_err(map)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn purge(
    State(svc): State<Arc<SiteCacheService>>,
    AuthUser(user, _): AuthUser,
    Json(body): Json<CdnPurgeBody>,
) -> ApiResult<Json<openpanel_app::site_cache_cdn::PurgeSummary>> {
    require_owner(&user)?;
    let summary = svc
        .purge_via_integration(user.username().as_str(), body.integration_id, body.paths)
        .await
        .map_err(map)?;
    Ok(Json(summary))
}

#[derive(Debug, Serialize)]
struct CachePolicyView {
    site_id: Uuid,
    ttl_seconds: u32,
    static_assets_ttl_seconds: u32,
    bypass_paths: Vec<String>,
    keyed_cookies: Vec<String>,
    stale_while_revalidate: bool,
    revalidation_required: bool,
}

impl From<SiteCachePolicy> for CachePolicyView {
    fn from(p: SiteCachePolicy) -> Self {
        Self {
            site_id: p.site_id(),
            ttl_seconds: p.ttl_seconds(),
            static_assets_ttl_seconds: p.static_assets_ttl_seconds(),
            bypass_paths: p.bypass_paths().to_vec(),
            keyed_cookies: p.keyed_cookies().to_vec(),
            stale_while_revalidate: p.stale_while_revalidate(),
            revalidation_required: p.revalidation_required(),
        }
    }
}

fn require_owner(user: &openpanel_domain::User) -> Result<(), ApiError> {
    if matches!(user.role(), openpanel_domain::Role::Owner) {
        Ok(())
    } else {
        Err(ApiError::Forbidden)
    }
}

fn map(error: openpanel_domain::SiteCacheCdnError) -> ApiError {
    use openpanel_domain::SiteCacheCdnError as E;
    match error {
        E::InvalidTtl(t) => ApiError::Unprocessable(format!("invalid TTL: {t}")),
        E::InvalidBypassPath(p) => ApiError::Unprocessable(format!("invalid bypass path: {p}")),
        E::TooManyBypassPaths(n) => ApiError::Unprocessable(format!("too many bypass paths: {n}")),
        E::InvalidKeyedCookie(c) => ApiError::Unprocessable(format!("invalid keyed cookie: {c}")),
        E::TooManyKeyedCookies(n) => {
            ApiError::Unprocessable(format!("too many keyed cookies: {n}"))
        }
        E::InvalidName(m) => ApiError::Unprocessable(format!("invalid name: {m}")),
        E::InvalidConfig(m) => ApiError::Unprocessable(format!("invalid config: {m}")),
        E::EmptyPurge => ApiError::Unprocessable("purge paths must not be empty".into()),
        E::TooManyPurgePaths(n) => ApiError::Unprocessable(format!("too many purge paths: {n}")),
        E::InvalidPurgePath(p) => ApiError::Unprocessable(format!("invalid purge path: {p}")),
        E::IntegrationNotFound => ApiError::NotFound("CDN integration not found".into()),
        E::PolicyNotFound => ApiError::NotFound("cache policy not found".into()),
        E::AdapterNotAvailable(k) => {
            ApiError::ServiceUnavailable(format!("no adapter registered for `{k}`"))
        }
        E::CryptoFailure(m) => ApiError::Unprocessable(format!("credential decode failed: {m}")),
        E::Adapter(m) => ApiError::Internal(format!("CDN adapter error: {m}")),
        E::Persistence(m) => ApiError::Internal(format!("cache+cdn persistence error: {m}")),
    }
}

// Tests for the request-side DTO validation and the error mapping
// live in `crates/openpanel-app/src/site_cache_cdn/service.rs` and
// `tests/integration/site_cache_cdn.rs`. The route file is
// integration-tested through the live `TestServer` harness.
