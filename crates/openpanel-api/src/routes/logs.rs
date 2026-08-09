//! Authorized log browsing, traffic, audit, and download routes.

use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Query, State},
    http::{HeaderMap, HeaderValue, header},
    response::IntoResponse,
    routing::get,
};
use openpanel_app::logs::{
    LogActor, LogReadQuery, LogService, LogServiceError, MAX_DOWNLOAD_BYTES, MAX_LOG_LINES,
    MAX_READ_BYTES, TRAFFIC_RETENTION_DAYS,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::{ApiError, ApiResult, AuthUser};

/// Build `/logs` API routes.
pub fn router(service: Arc<LogService>) -> Router {
    Router::new()
        .route("/sources", get(sources))
        .route("/entries", get(entries))
        .route("/traffic", get(traffic))
        .route("/audit", get(audit))
        .route("/retention", get(retention))
        .route("/download", get(download))
        .with_state(service)
}

async fn retention(AuthUser(_, _): AuthUser) -> impl IntoResponse {
    Json(serde_json::json!({
        "traffic_days": TRAFFIC_RETENTION_DAYS,
        "max_read_bytes": MAX_READ_BYTES,
        "max_lines": MAX_LOG_LINES,
        "max_download_bytes": MAX_DOWNLOAD_BYTES
    }))
}

fn actor(user: &openpanel_domain::User) -> LogActor {
    LogActor::new(user.id(), user.role())
}

async fn sources(
    State(service): State<Arc<LogService>>,
    AuthUser(user, _): AuthUser,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(service.sources(actor(&user)).await.map_err(map)?))
}

#[derive(Debug, Deserialize)]
struct EntryQuery {
    source_id: Option<Uuid>,
    limit: Option<usize>,
    text: Option<String>,
    cursor: Option<String>,
    severity: Option<String>,
    status: Option<u16>,
    since: Option<chrono::DateTime<chrono::Utc>>,
    until: Option<chrono::DateTime<chrono::Utc>>,
}

async fn entries(
    State(service): State<Arc<LogService>>,
    AuthUser(user, _): AuthUser,
    Query(input): Query<EntryQuery>,
) -> ApiResult<impl IntoResponse> {
    let source_id = input
        .source_id
        .ok_or_else(|| ApiError::BadRequest("source_id is required".into()))?;
    let mut query = LogReadQuery::new(source_id, input.limit.unwrap_or(100));
    query.text = input.text;
    query.cursor = input.cursor;
    query.severity = input.severity;
    query.status = input.status;
    query.since = input.since;
    query.until = input.until;
    Ok(Json(
        service.entries(actor(&user), query).await.map_err(map)?,
    ))
}

async fn traffic(
    State(service): State<Arc<LogService>>,
    AuthUser(user, _): AuthUser,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(service.traffic(actor(&user)).await.map_err(map)?))
}

#[derive(Debug, Deserialize)]
struct AuditQuery {
    limit: Option<usize>,
}
async fn audit(
    State(service): State<Arc<LogService>>,
    AuthUser(user, _): AuthUser,
    Query(input): Query<AuditQuery>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(
        service
            .audit_events(actor(&user), input.limit.unwrap_or(100))
            .await
            .map_err(map)?,
    ))
}

#[derive(Debug, Deserialize)]
struct DownloadQuery {
    source_id: Option<Uuid>,
    max_bytes: Option<u64>,
}
async fn download(
    State(service): State<Arc<LogService>>,
    AuthUser(user, _): AuthUser,
    Query(input): Query<DownloadQuery>,
) -> ApiResult<impl IntoResponse> {
    let source_id = input
        .source_id
        .ok_or_else(|| ApiError::BadRequest("source_id is required".into()))?;
    let bytes = service
        .download(
            actor(&user),
            source_id,
            input.max_bytes.unwrap_or(MAX_DOWNLOAD_BYTES),
        )
        .await
        .map_err(map)?;
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/plain; charset=utf-8"),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_static("attachment; filename= openpanel-log.txt"),
    );
    Ok((headers, bytes))
}

fn map(error: LogServiceError) -> ApiError {
    match error {
        LogServiceError::NotFound => ApiError::NotFound("log source".into()),
        LogServiceError::Forbidden => ApiError::Forbidden,
        LogServiceError::LimitExceeded => ApiError::PayloadTooLarge(MAX_DOWNLOAD_BYTES),
        LogServiceError::Validation(message) => ApiError::BadRequest(message),
        LogServiceError::Storage(_) => ApiError::Internal("log storage unavailable".into()),
    }
}
