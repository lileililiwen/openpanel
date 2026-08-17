//! Plugin extension framework HTTP routes.
//!
//! Mounted under `/api/v1`:
//!
//! * `GET  /plugins`              — list installed plugins
//! * `POST /plugins`              — install a signed manifest
//! * `GET  /plugins/{id}`         — plugin detail
//! * `POST /plugins/{id}/enable`  — enable
//! * `POST /plugins/{id}/disable` — disable
//! * `DELETE /plugins/{id}`       — uninstall

use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};
use openpanel_app::{PluginService, plugin::service::PluginInstallError};
use openpanel_domain::{PluginError, PluginId, PluginManifest, PluginRecord, PluginStatus};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{ApiError, ApiResult, AuthUser};

/// Build the routes.
pub fn router(service: Arc<PluginService>) -> Router {
    Router::new()
        .route("/plugins", get(list).post(install))
        .route("/plugins/{id}", get(detail).delete(uninstall))
        .route("/plugins/{id}/enable", post(enable))
        .route("/plugins/{id}/disable", post(disable))
        .with_state(service)
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct InstallBody {
    manifest: PluginManifest,
}

#[derive(Debug, Serialize)]
struct PluginView {
    id: String,
    version: String,
    publisher: String,
    status: String,
    installed_at: chrono::DateTime<chrono::Utc>,
    enabled_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl From<&PluginRecord> for PluginView {
    fn from(r: &PluginRecord) -> Self {
        Self {
            id: r.id.as_str().to_string(),
            version: r.version.as_str().to_string(),
            publisher: r.publisher.as_str().to_string(),
            status: match r.status {
                PluginStatus::Installed => "installed",
                PluginStatus::Enabled => "enabled",
                PluginStatus::Disabled => "disabled",
                PluginStatus::Failed => "failed",
            }
            .to_string(),
            installed_at: r.installed_at,
            enabled_at: r.enabled_at,
        }
    }
}

async fn install(
    State(svc): State<Arc<PluginService>>,
    AuthUser(user, _): AuthUser,
    Json(body): Json<InstallBody>,
) -> ApiResult<impl IntoResponse> {
    require_owner(&user)?;
    svc.install_manifest(&body.manifest, user.username().as_str())
        .await
        .map_err(map)?;
    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({
            "id": body.manifest.id.as_str(),
            "version": body.manifest.version.as_str(),
        })),
    ))
}

async fn list(
    State(svc): State<Arc<PluginService>>,
    AuthUser(user, _): AuthUser,
) -> ApiResult<Json<Vec<PluginView>>> {
    require_owner(&user)?;
    let rows = svc.list().await.map_err(map)?;
    Ok(Json(rows.iter().map(PluginView::from).collect()))
}

async fn detail(
    State(svc): State<Arc<PluginService>>,
    AuthUser(user, _): AuthUser,
    Path(id): Path<String>,
) -> ApiResult<Json<PluginView>> {
    require_owner(&user)?;
    let id = PluginId::new(id).map_err(map_plugin)?;
    let row = svc
        .find(&id)
        .await
        .map_err(map)?
        .ok_or_else(|| ApiError::NotFound("plugin not found".into()))?;
    Ok(Json(PluginView::from(&row)))
}

async fn enable(
    State(svc): State<Arc<PluginService>>,
    AuthUser(user, _): AuthUser,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    require_owner(&user)?;
    let id = PluginId::new(id).map_err(map_plugin)?;
    svc.enable(&id, user.username().as_str())
        .await
        .map_err(map)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn disable(
    State(svc): State<Arc<PluginService>>,
    AuthUser(user, _): AuthUser,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    require_owner(&user)?;
    let id = PluginId::new(id).map_err(map_plugin)?;
    svc.disable(&id, user.username().as_str())
        .await
        .map_err(map)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn uninstall(
    State(svc): State<Arc<PluginService>>,
    AuthUser(user, _): AuthUser,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    require_owner(&user)?;
    let id = PluginId::new(id).map_err(map_plugin)?;
    svc.uninstall(&id, user.username().as_str())
        .await
        .map_err(map)?;
    Ok(StatusCode::NO_CONTENT)
}

fn require_owner(user: &openpanel_domain::User) -> Result<(), ApiError> {
    if matches!(user.role(), openpanel_domain::Role::Owner) {
        Ok(())
    } else {
        Err(ApiError::Forbidden)
    }
}

fn map_plugin(error: PluginError) -> ApiError {
    match error {
        PluginError::InvalidManifest(m) => {
            ApiError::Unprocessable(format!("invalid manifest: {m}"))
        }
        PluginError::InvalidManifestSignature => {
            ApiError::Unprocessable("invalid manifest signature".into())
        }
        PluginError::AlreadyInstalled(m) => ApiError::Conflict(m),
        PluginError::NotInstalled(m) => ApiError::NotFound(m),
        other => ApiError::Unprocessable(other.to_string()),
    }
}

fn map(error: PluginInstallError) -> ApiError {
    use PluginInstallError as E;
    match error {
        E::Invalid(e) => match e {
            PluginError::InvalidManifest(m) => {
                ApiError::Unprocessable(format!("invalid manifest: {m}"))
            }
            PluginError::InvalidManifestSignature => {
                ApiError::Unprocessable("invalid manifest signature".into())
            }
            PluginError::AlreadyInstalled(m) => ApiError::Conflict(m),
            other => ApiError::Unprocessable(other.to_string()),
        },
        E::Persistence(m) => ApiError::Internal(m),
    }
}

// The `Uuid` import keeps the DTO surface explicit for future
// route extensions (e.g. scan ids); it is unused today.
#[allow(dead_code)]
fn _uuid(_: Uuid) {}
