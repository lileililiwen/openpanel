//! Web application installer HTTP routes.
//!
//! Mounted under `/api/v1`:
//!
//! * `POST /sites/{id}/web-apps/preview`  — build a typed `InstallPlan`
//! * `POST /sites/{id}/web-apps/run`      — confirm + run an install
//! * `DELETE /web-apps/{id}`              — uninstall (requires confirmed_at)
//! * `GET  /sites/{id}/web-apps`          — list installed web apps

use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, post},
};
use openpanel_app::web_application_installer::WebApplicationInstallerService;
use openpanel_domain::{InstallDb, WebApplicationInstallerError};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{ApiError, ApiResult, AuthUser};

/// Build the routes.
pub fn router(service: Arc<WebApplicationInstallerService>) -> Router {
    Router::new()
        .route(
            "/sites/{site_id}/web-apps",
            get(list_installed).post(create_install),
        )
        .route("/sites/{site_id}/web-apps/preview", post(preview_plan))
        .route("/sites/{site_id}/web-apps/run", post(run_install))
        .route("/web-apps/{install_id}", delete(uninstall))
        .with_state(service)
}

// ---- DTOs ---------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateInstallBody {
    app_id: String,
    install_path: String,
    #[serde(default)]
    requires_db: bool,
    #[serde(default)]
    db_name: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PreviewBody {
    app_id: String,
    install_path: String,
    #[serde(default)]
    requires_db: bool,
    #[serde(default)]
    db_name: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RunInstallBody {
    plan_id: Uuid,
    confirmed_at: chrono::DateTime<chrono::Utc>,
    #[serde(default)]
    idempotency_key: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct UninstallQuery {
    confirmed_at: chrono::DateTime<chrono::Utc>,
    #[serde(default)]
    drop_db: bool,
}

#[derive(Debug, Serialize)]
struct PlanView {
    plan_id: Uuid,
    app_id: String,
    site_id: Uuid,
    install_path: String,
    artifacts: usize,
    overlays: usize,
    content_hash: String,
    expires_at: chrono::DateTime<chrono::Utc>,
    warnings: Vec<String>,
}

#[derive(Debug, Serialize)]
struct RunView {
    run_id: Uuid,
    install_id: String,
    install_path: String,
    post_install_url: Option<String>,
    started_at: chrono::DateTime<chrono::Utc>,
    finished_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Serialize)]
struct InstalledView {
    install_id: String,
    site_id: Uuid,
    app_id: String,
    version: String,
    install_path: String,
    created_at: chrono::DateTime<chrono::Utc>,
    removed_at: Option<chrono::DateTime<chrono::Utc>>,
}

// ---- handlers -----------------------------------------------------------

async fn preview_plan(
    State(svc): State<Arc<WebApplicationInstallerService>>,
    AuthUser(user, _): AuthUser,
    Path(site_id): Path<Uuid>,
    Json(body): Json<PreviewBody>,
) -> ApiResult<Json<PlanView>> {
    require_owner(&user)?;
    let db = if body.requires_db {
        InstallDb::Fresh {
            name: body.db_name.unwrap_or_else(|| body.app_id.clone()),
        }
    } else {
        InstallDb::None
    };
    let plan = svc
        .plan(
            user.username().as_str(),
            body.app_id,
            site_id,
            body.install_path,
            vec![],
            db,
            vec![],
            vec!["preview-only; no artifacts are fetched".to_string()],
        )
        .await
        .map_err(map)?;
    Ok(Json(PlanView {
        plan_id: plan.id(),
        app_id: plan.app_id().to_string(),
        site_id: plan.site_id(),
        install_path: plan.install_path().to_string(),
        artifacts: plan.artifacts().len(),
        overlays: plan.overlays().len(),
        content_hash: plan.content_hash().to_string(),
        expires_at: plan.expires_at(),
        warnings: plan.warnings().to_vec(),
    }))
}

async fn create_install(
    State(svc): State<Arc<WebApplicationInstallerService>>,
    AuthUser(user, _): AuthUser,
    Path(site_id): Path<Uuid>,
    Json(body): Json<CreateInstallBody>,
) -> ApiResult<impl IntoResponse> {
    require_owner(&user)?;
    let db = if body.requires_db {
        InstallDb::Fresh {
            name: body.db_name.unwrap_or_else(|| body.app_id.clone()),
        }
    } else {
        InstallDb::None
    };
    let plan = svc
        .plan(
            user.username().as_str(),
            body.app_id,
            site_id,
            body.install_path,
            vec![],
            db,
            vec![],
            vec![],
        )
        .await
        .map_err(map)?;
    Ok((
        StatusCode::CREATED,
        Json(PlanView {
            plan_id: plan.id(),
            app_id: plan.app_id().to_string(),
            site_id: plan.site_id(),
            install_path: plan.install_path().to_string(),
            artifacts: plan.artifacts().len(),
            overlays: plan.overlays().len(),
            content_hash: plan.content_hash().to_string(),
            expires_at: plan.expires_at(),
            warnings: plan.warnings().to_vec(),
        }),
    ))
}

async fn run_install(
    State(svc): State<Arc<WebApplicationInstallerService>>,
    AuthUser(user, _): AuthUser,
    Path(_site_id): Path<Uuid>,
    Json(body): Json<RunInstallBody>,
) -> ApiResult<Json<RunView>> {
    require_owner(&user)?;
    let run = svc
        .run(
            user.username().as_str(),
            body.plan_id,
            body.confirmed_at,
            body.idempotency_key.as_deref(),
        )
        .await
        .map_err(map)?;
    Ok(Json(RunView {
        run_id: run.id(),
        install_id: run.install_id().to_string(),
        install_path: run.install_path().to_string(),
        post_install_url: run.post_install_url().map(str::to_owned),
        started_at: run.started_at(),
        finished_at: run.finished_at(),
    }))
}

async fn list_installed(
    State(svc): State<Arc<WebApplicationInstallerService>>,
    AuthUser(user, _): AuthUser,
    Path(site_id): Path<Uuid>,
) -> ApiResult<Json<Vec<InstalledView>>> {
    require_owner(&user)?;
    let rows = svc.list_installed(site_id).await.map_err(map)?;
    Ok(Json(
        rows.into_iter()
            .map(|i| InstalledView {
                install_id: i.install_id().to_string(),
                site_id: i.site_id(),
                app_id: i.app_id().to_string(),
                version: i.version().to_string(),
                install_path: i.install_path().to_string(),
                created_at: i.created_at(),
                removed_at: i.removed_at(),
            })
            .collect(),
    ))
}

async fn uninstall(
    State(svc): State<Arc<WebApplicationInstallerService>>,
    AuthUser(user, _): AuthUser,
    Path(install_id): Path<String>,
    Query(query): Query<UninstallQuery>,
) -> ApiResult<StatusCode> {
    require_owner(&user)?;
    let now = chrono::Utc::now();
    let delta = (now - query.confirmed_at).num_seconds().abs();
    if delta > openpanel_domain::CONFIRM_WINDOW_SECONDS {
        return Err(ApiError::BadRequest(
            "confirmed_at is outside the 60s window".into(),
        ));
    }
    svc.uninstall(user.username().as_str(), &install_id, query.drop_db)
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

fn map(error: WebApplicationInstallerError) -> ApiError {
    use WebApplicationInstallerError as E;
    match error {
        E::PlanExpired => ApiError::Unprocessable("install plan expired".into()),
        E::InvalidAppId => ApiError::Unprocessable("invalid app id".into()),
        E::InstallArtifactRejected(m) => ApiError::Unprocessable(format!("artifact rejected: {m}")),
        E::ArtifactSignatureInvalid => ApiError::Unprocessable("artifact signature invalid".into()),
        E::MissingConfirmation => ApiError::BadRequest("missing confirmation".into()),
        E::InstallInFlight => ApiError::Conflict("install already in flight".into()),
        E::AlreadyInstalled => ApiError::Conflict("app already installed at the same path".into()),
        E::OverlayDiffRequiresConfirmation(_) => {
            ApiError::Conflict("overlay diff requires destructive confirmation".into())
        }
        E::SiteNotFound => ApiError::NotFound("site not found".into()),
        E::ManifestNotFound => ApiError::NotFound("manifest not found".into()),
        E::Forbidden => ApiError::Forbidden,
        E::InstallNotFound => ApiError::NotFound("install not found".into()),
        E::Refused(m) => ApiError::BadRequest(m),
        E::Persistence(m) => ApiError::Internal(m),
    }
}
