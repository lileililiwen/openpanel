//! Sites HTTP routes.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use openpanel_app::SitesService;
use openpanel_domain::SiteError;
use uuid::Uuid;

use crate::dto::{CreateSiteRequest, PatchSiteRequest, SiteDto};
use crate::error::{ApiError, ApiResult};
use crate::extract::AuthUser;

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
        SiteError::DuplicateDomain(_) => ApiError::Conflict(e.to_string()),
        SiteError::NginxTest(_)
        | SiteError::NginxReload(_)
        | SiteError::NginxMissing
        | SiteError::Io(_)
        | SiteError::Persistence(_) => ApiError::Internal(e.to_string()),
    }
}
