//! Owner-only per-site HTTP-controls routes.

use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, State},
    routing::get,
};
use openpanel_app::SiteHttpService;
use openpanel_domain::site_http_controls::{SiteHttpControls, SiteHttpError};
use uuid::Uuid;

use crate::{ApiError, ApiResult, AuthUser};

/// Build routes nested under `/sites`.
pub fn router(service: Arc<SiteHttpService>) -> Router {
    Router::new()
        .route("/{id}/http", get(get_controls).put(put_controls))
        .with_state(service)
}

async fn get_controls(
    State(service): State<Arc<SiteHttpService>>,
    AuthUser(user, _): AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<SiteHttpControls>> {
    Ok(Json(service.get(&user, id).await.map_err(map)?))
}

async fn put_controls(
    State(service): State<Arc<SiteHttpService>>,
    AuthUser(user, _): AuthUser,
    Path(id): Path<Uuid>,
    Json(controls): Json<SiteHttpControls>,
) -> ApiResult<Json<SiteHttpControls>> {
    // The path parameter is authoritative; ignore any body site_id.
    let controls = rewrite_site_id(controls, id);
    Ok(Json(service.put(&user, controls).await.map_err(map)?))
}

fn rewrite_site_id(controls: SiteHttpControls, site_id: Uuid) -> SiteHttpControls {
    use openpanel_domain::site_http_controls::SiteHttpControlsInput;
    let version = controls.version();
    let input = SiteHttpControlsInput {
        error_pages: controls.error_pages().to_vec(),
        redirects: controls.redirects().to_vec(),
        protected_dirs: controls.protected_dirs().to_vec(),
        hotlink: controls.hotlink().cloned(),
        ip_rules: controls.ip_rules().to_vec(),
        mime_overrides: controls.mime_overrides().to_vec(),
        index_policy: controls.index_policy().cloned(),
    };
    // On invalid input keep the original document so the service's
    // validation reports the real error.
    SiteHttpControls::new(site_id, version, input).unwrap_or(controls)
}

fn map(error: SiteHttpError) -> ApiError {
    match error {
        SiteHttpError::Forbidden => ApiError::Forbidden,
        SiteHttpError::SiteNotFound(message) => ApiError::NotFound(message),
        SiteHttpError::Invalid(message) => ApiError::Unprocessable(message),
        SiteHttpError::PathOutsideSite => ApiError::Unprocessable("path_outside_site".into()),
        SiteHttpError::RedirectLoop => ApiError::Unprocessable("redirect_loop".into()),
        SiteHttpError::Persistence(message) => ApiError::Internal(message),
    }
}
