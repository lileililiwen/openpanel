//! Container runtime HTTP routes.
//!
//! Six endpoint groups under `/container` (per the spec the
//! paths include `/container-quota`, `/containers/{id}/metrics`,
//! `/containers/{id}/pull`, `/containers/{id}/egress/limit`,
//! and `/registry/credentials`):
//!
//! * `GET    /container/quota`                       show the
//!   caller's effective container quota (with plan overrides).
//! * `PUT    /container/quota`                       update the
//!   per-user quota axes (Owner/Admin only, plan caps still
//!   applied).
//! * `POST   /containers/{id}/metrics`               append a
//!   container metrics sample.
//! * `GET    /containers/{id}/metrics`               list the
//!   latest samples.
//! * `POST   /containers/{id}/pull`                 pull an
//!   image on behalf of a container.
//! * `PUT    /containers/{id}/egress/limit`         raise the
//!   user's monthly egress limit.
//! * `POST   /registry/credentials`                 create a
//!   registry credential (plaintext returned once).
//! * `GET    /registry/credentials`                 list the
//!   caller's credentials (redacted).
//! * `DELETE /registry/credentials/{id}`            delete a
//!   credential.
//!
//! Note: the spec lists paths under `/api/v1/...`; this router
//! is nested under `/container` so the compose root mounts
//! `Router::new().nest("/container", container_runtime_router(...))`.

use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get, post, put},
};
use openpanel_app::container_runtime::{
    ContainerRuntimeService, CreateCredentialResult, PullError, QuotaUpdate,
};
use openpanel_domain::{ContainerMetrics, ContainerRuntimeError, QuotaAxis};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{ApiError, ApiResult, AuthUser};

/// Build the `/container` routes.
pub fn router(service: Arc<ContainerRuntimeService>) -> Router {
    Router::new()
        .route("/quota", get(show_quota).put(set_quota))
        .route(
            "/containers/{id}/metrics",
            post(record_metrics).get(list_metrics),
        )
        .route("/containers/{id}/pull", post(pull_image))
        .route("/containers/{id}/egress/limit", put(raise_egress_limit))
        .route(
            "/registry/credentials",
            post(create_credential).get(list_credentials),
        )
        .route("/registry/credentials/{id}", delete(delete_credential))
        .with_state(service)
}

async fn show_quota(
    State(svc): State<Arc<ContainerRuntimeService>>,
    AuthUser(user, _): AuthUser,
) -> ApiResult<Json<EffectiveQuotaView>> {
    let eff = svc
        .get_quota(&user, user.id())
        .await
        .map_err(map_runtime_error)?;
    Ok(Json(EffectiveQuotaView::from(&eff)))
}

#[derive(Debug, Deserialize)]
struct QuotaBody {
    max_concurrent: Option<u32>,
    max_total: Option<u32>,
    cpu_pct_max: Option<u8>,
    memory_bytes_max: Option<u64>,
    egress_bytes_per_month: Option<u64>,
}

async fn set_quota(
    State(svc): State<Arc<ContainerRuntimeService>>,
    AuthUser(user, _): AuthUser,
    Json(body): Json<QuotaBody>,
) -> ApiResult<Json<EffectiveQuotaView>> {
    let update = QuotaUpdate {
        max_concurrent: body.max_concurrent,
        max_total: body.max_total,
        cpu_pct_max: body.cpu_pct_max,
        memory_bytes_max: body.memory_bytes_max,
        egress_bytes_per_month: body.egress_bytes_per_month,
    };
    let eff = svc
        .set_quota(&user, user.id(), update)
        .await
        .map_err(map_runtime_error)?;
    Ok(Json(EffectiveQuotaView::from(&eff)))
}

#[derive(Debug, Deserialize)]
struct RecordMetricsBody {
    cpu_pct: u16,
    memory_bytes: u64,
    net_rx: u64,
    net_tx: u64,
    exits: u32,
}

async fn record_metrics(
    State(svc): State<Arc<ContainerRuntimeService>>,
    AuthUser(user, _): AuthUser,
    Path(container_id): Path<Uuid>,
    Json(body): Json<RecordMetricsBody>,
) -> ApiResult<StatusCode> {
    let now = chrono::Utc::now();
    let metrics = ContainerMetrics {
        user_id: user.id(),
        container_id,
        cpu_pct: body.cpu_pct,
        memory_bytes: body.memory_bytes,
        net_rx: body.net_rx,
        net_tx: body.net_tx,
        exits: body.exits,
        sampled_at: now,
    };
    svc.record_metrics(&user, metrics)
        .await
        .map_err(map_runtime_error)?;
    Ok(StatusCode::CREATED)
}

#[derive(Debug, Deserialize)]
struct ListMetricsQuery {
    limit: Option<u32>,
}

async fn list_metrics(
    State(svc): State<Arc<ContainerRuntimeService>>,
    AuthUser(user, _): AuthUser,
    Path(container_id): Path<Uuid>,
    axum::extract::Query(q): axum::extract::Query<ListMetricsQuery>,
) -> ApiResult<Json<Vec<MetricsView>>> {
    let list = svc
        .list_metrics(&user, container_id, q.limit.unwrap_or(60))
        .await
        .map_err(map_runtime_error)?;
    Ok(Json(list.iter().map(MetricsView::from).collect()))
}

#[derive(Debug, Deserialize)]
struct PullBody {
    image_ref: String,
    registry_credential_id: Option<Uuid>,
}

#[derive(Debug, Serialize)]
struct PullResultView {
    image_digest: String,
    ref_count: u32,
}

async fn pull_image(
    State(svc): State<Arc<ContainerRuntimeService>>,
    AuthUser(user, _): AuthUser,
    Path(container_id): Path<Uuid>,
    Json(body): Json<PullBody>,
) -> ApiResult<Json<PullResultView>> {
    let result = svc
        .pull_image(
            &user,
            container_id,
            body.image_ref,
            body.registry_credential_id,
        )
        .await
        .map_err(map_pull_error)?;
    Ok(Json(PullResultView {
        image_digest: result.image_digest,
        ref_count: result.ref_count,
    }))
}

#[derive(Debug, Deserialize)]
struct RaiseEgressBody {
    bytes_per_month: u64,
}

async fn raise_egress_limit(
    State(svc): State<Arc<ContainerRuntimeService>>,
    AuthUser(user, _): AuthUser,
    Path(_container_id): Path<Uuid>,
    Json(body): Json<RaiseEgressBody>,
) -> ApiResult<Json<QuotaView>> {
    let quota = svc
        .raise_egress_limit(&user, user.id(), body.bytes_per_month)
        .await
        .map_err(map_runtime_error)?;
    Ok(Json(QuotaView::from(&quota)))
}

#[derive(Debug, Deserialize)]
struct CreateCredentialBody {
    registry: String,
    username: String,
    password: String,
}

async fn create_credential(
    State(svc): State<Arc<ContainerRuntimeService>>,
    AuthUser(user, _): AuthUser,
    Json(body): Json<CreateCredentialBody>,
) -> ApiResult<(StatusCode, Json<CreateCredentialResultView>)> {
    let r = svc
        .create_registry_credential(&user, body.registry, body.username, body.password)
        .await
        .map_err(map_runtime_error)?;
    Ok((
        StatusCode::CREATED,
        Json(CreateCredentialResultView::from(&r)),
    ))
}

async fn list_credentials(
    State(svc): State<Arc<ContainerRuntimeService>>,
    AuthUser(user, _): AuthUser,
) -> ApiResult<Json<Vec<CredentialView>>> {
    let list = svc
        .list_registry_credentials(&user, user.id())
        .await
        .map_err(map_runtime_error)?;
    Ok(Json(list.iter().map(CredentialView::from).collect()))
}

async fn delete_credential(
    State(svc): State<Arc<ContainerRuntimeService>>,
    AuthUser(user, _): AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    let removed = svc
        .delete_registry_credential(&user, id)
        .await
        .map_err(map_runtime_error)?;
    Ok(Json(serde_json::json!({"removed": removed})))
}

fn map_pull_error(e: PullError) -> ApiError {
    match e {
        PullError::AuthFailed => ApiError::Forbidden,
        PullError::BadImageRef(m) => ApiError::Unprocessable(m),
        PullError::Unavailable(m) => ApiError::ServiceUnavailable(m),
        PullError::Other(m) => ApiError::Internal(m),
    }
}

fn map_runtime_error(e: ContainerRuntimeError) -> ApiError {
    match e {
        ContainerRuntimeError::Forbidden => ApiError::Forbidden,
        ContainerRuntimeError::QuotaExceeded { axis: _ } => {
            ApiError::Unprocessable("quota exceeded".into())
        }
        ContainerRuntimeError::CredentialDecode => {
            ApiError::Unprocessable("password rejected".into())
        }
        ContainerRuntimeError::InvalidCipher(m) => ApiError::Internal(m),
        ContainerRuntimeError::AdapterUnavailable(m) => ApiError::ServiceUnavailable(m),
        ContainerRuntimeError::CredentialNotFound(m) => ApiError::NotFound(m),
        ContainerRuntimeError::Persistence(m) => ApiError::Internal(m),
    }
}

// ---- views ----

#[derive(Debug, Serialize)]
struct EffectiveQuotaView {
    quota: QuotaView,
    plan_overrides: Vec<String>,
}

impl From<&openpanel_domain::EffectiveQuota> for EffectiveQuotaView {
    fn from(e: &openpanel_domain::EffectiveQuota) -> Self {
        Self {
            quota: QuotaView::from(&e.quota),
            plan_overrides: e
                .plan_overrides
                .iter()
                .map(|axis| match axis {
                    QuotaAxis::Concurrent => "concurrent".into(),
                    QuotaAxis::Total => "total".into(),
                    QuotaAxis::Cpu => "cpu".into(),
                    QuotaAxis::Memory => "memory".into(),
                    QuotaAxis::Egress => "egress".into(),
                })
                .collect(),
        }
    }
}

#[derive(Debug, Serialize)]
struct QuotaView {
    user_id: String,
    max_concurrent: u32,
    max_total: u32,
    cpu_pct_max: u8,
    memory_bytes_max: u64,
    egress_bytes_per_month: u64,
    updated_at: String,
}

impl From<&openpanel_domain::ContainerQuota> for QuotaView {
    fn from(q: &openpanel_domain::ContainerQuota) -> Self {
        Self {
            user_id: q.user_id.to_string(),
            max_concurrent: q.max_concurrent,
            max_total: q.max_total,
            cpu_pct_max: q.cpu_pct_max,
            memory_bytes_max: q.memory_bytes_max,
            egress_bytes_per_month: q.egress_bytes_per_month,
            updated_at: q.updated_at.to_rfc3339(),
        }
    }
}

#[derive(Debug, Serialize)]
struct MetricsView {
    cpu_pct: u16,
    memory_bytes: u64,
    net_rx: u64,
    net_tx: u64,
    exits: u32,
    sampled_at: String,
}

impl From<&ContainerMetrics> for MetricsView {
    fn from(m: &ContainerMetrics) -> Self {
        Self {
            cpu_pct: m.cpu_pct,
            memory_bytes: m.memory_bytes,
            net_rx: m.net_rx,
            net_tx: m.net_tx,
            exits: m.exits,
            sampled_at: m.sampled_at.to_rfc3339(),
        }
    }
}

#[derive(Debug, Serialize)]
struct CredentialView {
    id: String,
    user_id: String,
    registry: String,
    username: String,
    // Always redacted in API responses (spec: Plaintext never echoed).
    encrypted_secret: String,
    created_at: String,
    last_used_at: Option<String>,
}

impl From<&openpanel_domain::RegistryCredential> for CredentialView {
    fn from(c: &openpanel_domain::RegistryCredential) -> Self {
        Self {
            id: c.id.to_string(),
            user_id: c.user_id.to_string(),
            registry: c.registry.clone(),
            username: c.username.clone(),
            encrypted_secret: c.encrypted_secret.clone(),
            created_at: c.created_at.to_rfc3339(),
            last_used_at: c.last_used_at.map(|t| t.to_rfc3339()),
        }
    }
}

#[derive(Debug, Serialize)]
struct CreateCredentialResultView {
    credential: CredentialView,
    // Plaintext secret returned exactly once. The caller (curl/UI)
    // MUST NOT persist or echo it back in any subsequent read.
    plaintext_once: String,
}

impl From<&CreateCredentialResult> for CreateCredentialResultView {
    fn from(r: &CreateCredentialResult) -> Self {
        Self {
            credential: CredentialView::from(&r.credential),
            plaintext_once: r.plaintext_once.clone(),
        }
    }
}
