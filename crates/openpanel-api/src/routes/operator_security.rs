//! Owner/admin security-findings control-plane API.
//!
//! Normalized queue, typed remediation preview/execute with
//! idempotency and post-check, suppression with expiry, and
//! secret-safe projections (evidence is redacted at the domain
//! boundary; audit metadata is redacted in the service).

use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};
use chrono::{DateTime, Utc};
use openpanel_app::{ControlPlaneServiceError, OperatorSecurityService};
use openpanel_domain::{
    Role,
    operator_security::{FindingSeverity, FindingSource, RemediationMode, SecurityFinding},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{ApiError, ApiResult, AuthUser};

/// Build `/security/findings` routes.
pub fn router(service: Arc<OperatorSecurityService>) -> Router {
    Router::new()
        .route("/", get(queue).post(ingest))
        .route("/{id}", get(detail))
        .route("/{id}/preview", get(preview))
        .route("/{id}/remediate", post(remediate))
        .route("/{id}/suppress", post(suppress))
        .with_state(service)
}

fn require_operator(user: &openpanel_domain::User) -> ApiResult<()> {
    if matches!(user.role(), Role::Owner | Role::Admin) {
        Ok(())
    } else {
        Err(ApiError::Forbidden)
    }
}

#[derive(Debug, Serialize)]
struct FindingView {
    id: Uuid,
    source: String,
    severity: String,
    resource: String,
    rule: String,
    title: String,
    evidence: String,
    remediation_mode: String,
    state: String,
    first_seen_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    suppression_expires_at: Option<DateTime<Utc>>,
}

impl From<SecurityFinding> for FindingView {
    fn from(item: SecurityFinding) -> Self {
        Self {
            id: item.id(),
            source: item.source().as_str().to_string(),
            severity: item.severity().as_str().to_string(),
            resource: item.resource().to_string(),
            rule: item.rule_key().to_string(),
            title: item.title().to_string(),
            evidence: item.evidence().to_string(),
            remediation_mode: item.remediation_mode().as_str().to_string(),
            state: item.state().as_str().to_string(),
            first_seen_at: item.first_seen_at(),
            updated_at: item.updated_at(),
            suppression_expires_at: item.suppression().map(|entry| entry.expires_at()),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct NewFinding {
    source: FindingSource,
    severity: FindingSeverity,
    resource: String,
    rule: String,
    title: String,
    evidence: String,
    remediation_mode: RemediationMode,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RemediateInput {
    idempotency_key: String,
    confirmed: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SuppressInput {
    reason: String,
    scope: String,
    expires_at: DateTime<Utc>,
}

async fn queue(
    State(service): State<Arc<OperatorSecurityService>>,
    AuthUser(user, _): AuthUser,
) -> ApiResult<Json<Vec<FindingView>>> {
    require_operator(&user)?;
    Ok(Json(
        service
            .queue(&user, Utc::now())
            .await
            .map_err(map)?
            .into_iter()
            .map(FindingView::from)
            .collect(),
    ))
}

async fn ingest(
    State(service): State<Arc<OperatorSecurityService>>,
    AuthUser(user, _): AuthUser,
    Json(input): Json<Vec<NewFinding>>,
) -> ApiResult<impl IntoResponse> {
    require_operator(&user)?;
    if input.is_empty() || input.len() > 200 {
        return Err(ApiError::Unprocessable(
            "ingest between 1 and 200 findings".into(),
        ));
    }
    let now = Utc::now();
    let mut findings = Vec::with_capacity(input.len());
    for item in input {
        findings.push(
            SecurityFinding::new(
                item.source,
                item.severity,
                item.resource,
                item.rule,
                item.title,
                item.evidence,
                item.remediation_mode,
                now,
            )
            .map_err(|error| ApiError::Unprocessable(error.to_string()))?,
        );
    }
    let stored = service.ingest(&user, findings, now).await.map_err(map)?;
    Ok((
        StatusCode::CREATED,
        Json(
            stored
                .into_iter()
                .map(FindingView::from)
                .collect::<Vec<_>>(),
        ),
    ))
}

async fn detail(
    State(service): State<Arc<OperatorSecurityService>>,
    AuthUser(user, _): AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<FindingView>> {
    require_operator(&user)?;
    Ok(Json(FindingView::from(
        service.find(&user, id).await.map_err(map)?,
    )))
}

async fn preview(
    State(service): State<Arc<OperatorSecurityService>>,
    AuthUser(user, _): AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    require_operator(&user)?;
    let info = service.preview(&user, id).await.map_err(map)?;
    Ok(Json(serde_json::json!({
        "finding_id": info.finding_id(),
        "adapter": info.kind().as_str(),
        "summary": info.summary(),
        "steps": info.steps(),
        "requires_confirmation": info.requires_confirmation(),
        "supports_rollback": info.supports_rollback(),
        "recovery_guidance": info.kind().recovery_guidance(),
    })))
}

async fn remediate(
    State(service): State<Arc<OperatorSecurityService>>,
    AuthUser(user, _): AuthUser,
    Path(id): Path<Uuid>,
    Json(input): Json<RemediateInput>,
) -> ApiResult<Json<FindingView>> {
    require_operator(&user)?;
    Ok(Json(FindingView::from(
        service
            .remediate(
                &user,
                id,
                &input.idempotency_key,
                input.confirmed,
                Utc::now(),
            )
            .await
            .map_err(map)?,
    )))
}

async fn suppress(
    State(service): State<Arc<OperatorSecurityService>>,
    AuthUser(user, _): AuthUser,
    Path(id): Path<Uuid>,
    Json(input): Json<SuppressInput>,
) -> ApiResult<Json<FindingView>> {
    require_operator(&user)?;
    Ok(Json(FindingView::from(
        service
            .suppress_finding(
                &user,
                id,
                &input.reason,
                &input.scope,
                input.expires_at,
                Utc::now(),
            )
            .await
            .map_err(map)?,
    )))
}

fn map(error: ControlPlaneServiceError) -> ApiError {
    match error {
        ControlPlaneServiceError::Forbidden => ApiError::Forbidden,
        ControlPlaneServiceError::NotFound => ApiError::NotFound("security finding".into()),
        ControlPlaneServiceError::Validation(message) => ApiError::Unprocessable(message),
        ControlPlaneServiceError::ConfirmationRequired => {
            ApiError::Conflict("remediation requires confirmation".into())
        }
        ControlPlaneServiceError::ManualRequired(guidance) => {
            ApiError::Unprocessable(format!("manual remediation required: {guidance}"))
        }
        ControlPlaneServiceError::Internal => {
            ApiError::Internal("control-plane unavailable".into())
        }
    }
}
