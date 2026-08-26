//! Sites HTTP routes.

use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{get, post},
};
use openpanel_app::SitesService;
use openpanel_domain::SiteError;
use uuid::Uuid;

use crate::{
    dto::{CreateSiteRequest, PatchSiteRequest, SiteDto},
    error::{ApiError, ApiResult},
    extract::AuthUser,
};

/// Builds the Axum sub-router for `/sites` routes.
pub fn router(svc: Arc<SitesService>) -> Router {
    Router::new()
        .route("/", get(list_sites).post(create_site))
        .route("/{id}", get(get_site).delete(delete_site).patch(patch_site))
        .route("/{id}/enable", post(enable_site))
        .route("/{id}/disable", post(disable_site))
        .with_state(svc)
}

async fn list_sites(
    State(svc): State<Arc<SitesService>>,
    AuthUser(user, _): AuthUser,
) -> ApiResult<Json<Vec<SiteDto>>> {
    let sites = svc.list_sites(&user).await.map_err(map_site_err)?;
    Ok(Json(sites.iter().map(SiteDto::from_site).collect()))
}

async fn create_site(
    State(svc): State<Arc<SitesService>>,
    AuthUser(user, _): AuthUser,
    Json(req): Json<CreateSiteRequest>,
) -> ApiResult<Json<SiteDto>> {
    let owner_id = req.owner_id.unwrap_or(user.id());
    let site = svc
        .create_site(
            &user,
            owner_id,
            &req.primary_domain,
            req.aliases,
            req.php_enabled,
            req.php_version,
            req.document_root,
        )
        .await
        .map_err(map_site_err)?;
    Ok(Json(SiteDto::from_site(&site)))
}

async fn get_site(
    State(svc): State<Arc<SitesService>>,
    AuthUser(_user, _): AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<SiteDto>> {
    let site = svc.get_site(id).await.map_err(map_site_err)?;
    Ok(Json(SiteDto::from_site(&site)))
}

async fn delete_site(
    State(svc): State<Arc<SitesService>>,
    AuthUser(user, _): AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    svc.delete_site(&user, id).await.map_err(map_site_err)?;
    Ok(Json(serde_json::json!({"ok": true})))
}

async fn enable_site(
    State(svc): State<Arc<SitesService>>,
    AuthUser(user, _): AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    svc.enable_site(&user, id).await.map_err(map_site_err)?;
    Ok(Json(serde_json::json!({"ok": true})))
}

async fn disable_site(
    State(svc): State<Arc<SitesService>>,
    AuthUser(user, _): AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    svc.disable_site(&user, id).await.map_err(map_site_err)?;
    Ok(Json(serde_json::json!({"ok": true})))
}

async fn patch_site(
    State(svc): State<Arc<SitesService>>,
    AuthUser(user, _): AuthUser,
    Path(id): Path<Uuid>,
    Json(req): Json<PatchSiteRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    if let Some(new_owner) = req.owner_id {
        svc.change_owner(&user, id, new_owner)
            .await
            .map_err(map_site_err)?;
    }
    if let Some(aliases) = req.aliases {
        svc.change_aliases(&user, id, aliases)
            .await
            .map_err(map_site_err)?;
    }
    Ok(Json(serde_json::json!({"ok": true})))
}

fn map_site_err(e: SiteError) -> ApiError {
    match e {
        SiteError::Forbidden => ApiError::Forbidden,
        SiteError::NotFound(_) => ApiError::NotFound(e.to_string()),
        SiteError::InvalidDomain(_)
        | SiteError::InvalidAlias(_, _)
        | SiteError::InvalidDocumentRoot(_) => ApiError::BadRequest(e.to_string()),
        SiteError::InvalidTransport(_) => ApiError::Unprocessable(e.to_string()),
        SiteError::DuplicateDomain(_) => ApiError::Conflict(e.to_string()),
        SiteError::NginxTest(_)
        | SiteError::NginxReload(_)
        | SiteError::NginxMissing
        | SiteError::Io(_)
        | SiteError::Persistence(_) => ApiError::Internal(e.to_string()),
    }
}

/// Builds the Axum sub-router for per-site transport tuning
/// (mounted beside the other `/sites` routers with its own state).
pub fn transport_router(svc: Arc<openpanel_app::SiteTransportService>) -> Router {
    Router::new()
        .route(
            "/{id}/transport",
            axum::routing::get(get_transport).put(put_transport),
        )
        .with_state(svc)
}

async fn get_transport(
    State(svc): State<Arc<openpanel_app::SiteTransportService>>,
    AuthUser(caller, _): AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    let policy = svc.get(&caller, id).await.map_err(map_site_err)?;
    Ok(Json(
        serde_json::to_value(&policy).map_err(|e| ApiError::Internal(e.to_string()))?,
    ))
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct TransportPolicyInput {
    http3_enabled: bool,
    tls_min_version: String,
    hsts: Option<HstsInput>,
    compression: CompressionInput,
    body_size_cap_bytes: u64,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct HstsInput {
    max_age_secs: u32,
    include_subdomains: bool,
    preload: bool,
}

#[derive(serde::Deserialize)]
#[serde(
    tag = "kind",
    content = "level",
    rename_all = "snake_case",
    deny_unknown_fields
)]
enum CompressionInput {
    Off,
    Gzip(u8),
    Brotli(u8),
}

async fn put_transport(
    State(svc): State<Arc<openpanel_app::SiteTransportService>>,
    AuthUser(caller, _): AuthUser,
    Path(id): Path<Uuid>,
    Json(input): Json<TransportPolicyInput>,
) -> ApiResult<Json<serde_json::Value>> {
    let tls_min_version = match input.tls_min_version.as_str() {
        "1.2" | "TLSv1.2" => openpanel_domain::TlsVersion::V1_2,
        "1.3" | "TLSv1.3" => openpanel_domain::TlsVersion::V1_3,
        other => {
            return Err(ApiError::BadRequest(format!(
                "unsupported tls_min_version `{other}` (use 1.2 or 1.3)"
            )));
        }
    };
    let hsts = match input.hsts {
        Some(hsts) => Some(
            openpanel_domain::HstsPolicy::new(
                hsts.max_age_secs,
                hsts.include_subdomains,
                hsts.preload,
            )
            .map_err(map_site_err)?,
        ),
        None => None,
    };
    let compression = match input.compression {
        CompressionInput::Off => openpanel_domain::CompressionPolicy::Off,
        CompressionInput::Gzip(level) => openpanel_domain::CompressionPolicy::Gzip(level),
        CompressionInput::Brotli(level) => openpanel_domain::CompressionPolicy::Brotli(level),
    };
    let policy = openpanel_domain::TransportPolicy::new(
        input.http3_enabled,
        tls_min_version,
        hsts,
        compression,
        openpanel_domain::ByteSize::new(input.body_size_cap_bytes).map_err(map_site_err)?,
    )
    .map_err(map_site_err)?;
    let saved = svc.set(&caller, id, policy).await.map_err(map_site_err)?;
    Ok(Json(
        serde_json::to_value(&saved).map_err(|e| ApiError::Internal(e.to_string()))?,
    ))
}
