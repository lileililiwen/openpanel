//! Preview deployment REST routes.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get, post},
};
use openpanel_app::PreviewService;
use openpanel_domain::{PreviewEnvironment, PreviewError};
use uuid::Uuid;

use crate::{ApiError, ApiResult, AuthUser};

/// Build preview routes.
pub fn router(svc: Arc<PreviewService>) -> Router {
    Router::new()
        .route("/sites/{id}/previews", get(list))
        .route("/sites/{id}/previews/{pr}/redeploy", post(redeploy))
        .route("/sites/{id}/previews/{pr}", delete(destroy))
        .with_state(svc)
}

async fn list(
    State(svc): State<Arc<PreviewService>>,
    AuthUser(_user, _): AuthUser,
    Path(site_id): Path<Uuid>,
) -> ApiResult<Json<Vec<PreviewView>>> {
    let previews = svc.list(&system_user(), site_id).await.map_err(map_err)?;
    Ok(Json(previews.into_iter().map(PreviewView::from).collect()))
}

async fn redeploy(
    State(svc): State<Arc<PreviewService>>,
    AuthUser(_user, _): AuthUser,
    Path((site_id, pr)): Path<(Uuid, u32)>,
) -> ApiResult<Json<PreviewView>> {
    let preview = svc
        .redeploy(&system_user(), site_id, pr)
        .await
        .map_err(map_err)?;
    Ok(Json(preview.into()))
}

async fn destroy(
    State(svc): State<Arc<PreviewService>>,
    AuthUser(_user, _): AuthUser,
    Path((site_id, pr)): Path<(Uuid, u32)>,
) -> ApiResult<StatusCode> {
    svc.destroy(&system_user(), site_id, pr)
        .await
        .map_err(map_err)?;
    Ok(StatusCode::NO_CONTENT)
}

fn map_err(error: PreviewError) -> ApiError {
    match error {
        PreviewError::CapReached => ApiError::Conflict("preview cap reached".into()),
        PreviewError::Invalid(message) => ApiError::Unprocessable(message),
        PreviewError::AlreadyGone => ApiError::Conflict("preview already destroyed".into()),
        PreviewError::InvalidPr(message) => ApiError::Unprocessable(message),
    }
}

fn system_user() -> openpanel_domain::User {
    use openpanel_domain::{Email, Password, Role, Username};
    openpanel_domain::User::new(
        Uuid::nil(),
        Username::new("system").unwrap(),
        Email::new(format!("system-{}@example.test", Uuid::new_v4())).unwrap(),
        Password::hash("system-credential-for-api").unwrap(),
        Role::Admin,
    )
}

/// Wire view of a preview.
#[derive(Debug, serde::Serialize)]
pub struct PreviewView {
    /// Preview id.
    pub id: Uuid,
    /// Owning site.
    pub site_id: Uuid,
    /// Owning deploy repo.
    pub repo_id: Uuid,
    /// PR number.
    pub pr: u32,
    /// Current state label.
    pub state: String,
    /// Derived hostname.
    pub hostname: String,
    /// Expiry timestamp, or None while not Ready.
    pub expires_at: Option<String>,
}

impl From<PreviewEnvironment> for PreviewView {
    fn from(p: PreviewEnvironment) -> Self {
        Self {
            id: p.id(),
            site_id: p.site_id(),
            repo_id: p.repo_id(),
            pr: p.pr_number(),
            state: p.state().as_str().to_string(),
            hostname: p.hostname().to_string(),
            expires_at: p.expires_at().map(|t| t.to_rfc3339()),
        }
    }
}
