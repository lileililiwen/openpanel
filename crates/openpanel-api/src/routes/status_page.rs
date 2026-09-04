//! Status page REST routes.
//!
//! Mounted under `/api/v1/status-page`:
//!
//! * `GET    /status-page`              — current policy (slug, enabled, entries)
//! * `PUT    /status-page`              — toggle enabled
//! * `POST   /status-page/regenerate-slug` — rotate slug
//! * `PUT    /status-page/entries/{check_id}` — set label + publish
//! * `DELETE /status-page/entries/{check_id}` — unpublish
//! * `GET    /status-page/incidents`    — list derived incidents

use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get, post, put},
};
use openpanel_app::StatusPageService;
use openpanel_domain::synthetic_monitoring::{Slug, StatusPage, StatusPageError};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{ApiError, ApiResult, AuthUser};

/// Build the routes.
pub fn router(service: Arc<StatusPageService>) -> Router {
    Router::new()
        .route("/status-page", get(get_policy))
        .route("/status-page", put(set_enabled))
        .route("/status-page/regenerate-slug", post(regenerate_slug))
        .route("/status-page/entries/{check_id}", put(publish_entry))
        .route("/status-page/entries/{check_id}", delete(unpublish_entry))
        .route("/status-page/incidents", get(list_incidents))
        .with_state(service)
}

#[derive(Debug, Serialize)]
struct StatusPageView {
    slug: String,
    enabled: bool,
    entries: Vec<EntryView>,
}

#[derive(Debug, Serialize)]
struct EntryView {
    check_id: Uuid,
    label: String,
}

impl From<&StatusPage> for StatusPageView {
    fn from(page: &StatusPage) -> Self {
        Self {
            slug: page.slug.as_str().to_string(),
            enabled: page.enabled,
            entries: page
                .entries
                .iter()
                .map(|e| EntryView {
                    check_id: e.check_id,
                    label: e.label.clone(),
                })
                .collect(),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SetEnabledBody {
    enabled: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PublishBody {
    label: String,
}

#[derive(Debug, Serialize)]
struct IncidentView {
    check_id: Uuid,
    started_at: chrono::DateTime<chrono::Utc>,
    resolved_at: Option<chrono::DateTime<chrono::Utc>>,
    outcome: String,
}

async fn get_policy(
    State(svc): State<Arc<StatusPageService>>,
    AuthUser(_user, _): AuthUser,
) -> ApiResult<Json<StatusPageView>> {
    let page = svc.get().await.map_err(map)?;
    Ok(Json(StatusPageView::from(&page)))
}

async fn set_enabled(
    State(svc): State<Arc<StatusPageService>>,
    AuthUser(user, _): AuthUser,
    Json(body): Json<SetEnabledBody>,
) -> ApiResult<Json<StatusPageView>> {
    let page = if body.enabled {
        svc.enable(&user).await.map_err(map)?
    } else {
        svc.disable(&user).await.map_err(map)?
    };
    Ok(Json(StatusPageView::from(&page)))
}

async fn regenerate_slug(
    State(svc): State<Arc<StatusPageService>>,
    AuthUser(user, _): AuthUser,
) -> ApiResult<Json<StatusPageView>> {
    let page = svc.regenerate_slug(&user).await.map_err(map)?;
    Ok(Json(StatusPageView::from(&page)))
}

async fn publish_entry(
    State(svc): State<Arc<StatusPageService>>,
    AuthUser(user, _): AuthUser,
    Path(check_id): Path<Uuid>,
    Json(body): Json<PublishBody>,
) -> ApiResult<Json<StatusPageView>> {
    let page = svc
        .publish(&user, check_id, body.label)
        .await
        .map_err(map)?;
    Ok(Json(StatusPageView::from(&page)))
}

async fn unpublish_entry(
    State(svc): State<Arc<StatusPageService>>,
    AuthUser(user, _): AuthUser,
    Path(check_id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    svc.unpublish(&user, check_id).await.map_err(map)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_incidents(
    State(svc): State<Arc<StatusPageService>>,
    AuthUser(_user, _): AuthUser,
) -> ApiResult<Json<Vec<IncidentView>>> {
    let view = svc.public_view().await.map_err(map)?;
    let incidents = view
        .incidents
        .into_iter()
        .map(|i| IncidentView {
            check_id: i.check_id,
            started_at: i.started_at,
            resolved_at: i.resolved_at,
            outcome: match i.outcome {
                openpanel_domain::CheckStatus::Ok => "ok",
                openpanel_domain::CheckStatus::Warn => "warn",
                openpanel_domain::CheckStatus::Fail => "fail",
            }
            .to_string(),
        })
        .collect();
    Ok(Json(incidents))
}

fn map(error: StatusPageError) -> ApiError {
    match error {
        StatusPageError::InvalidSlug => ApiError::Unprocessable("invalid slug".into()),
        StatusPageError::CheckNotFound(id) => ApiError::NotFound(format!("check not found: {id}")),
        StatusPageError::Disabled => ApiError::NotFound("status page disabled".into()),
        StatusPageError::Persistence(message) => {
            if message == "forbidden" {
                ApiError::Forbidden
            } else if message == "empty label" {
                ApiError::Unprocessable("label must not be empty".into())
            } else {
                ApiError::Internal(message)
            }
        }
    }
}

#[allow(dead_code)]
fn _ensure_slug(value: &str) -> Result<Slug, StatusPageError> {
    Slug::new(value)
}
