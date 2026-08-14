//! Container registry HTTP routes.
//!
//! Four endpoints, all under `/registry`:
//!
//! * `GET  /registry/config`         show registry configuration
//! * `PUT  /registry/config`         update retention / scan_on_push
//! * `POST /registry/namespaces`     create a per-user namespace
//! * `GET  /registry/namespaces`     list namespaces
//! * `POST /registry/images`         push hook (storage + record)
//! * `GET  /registry/namespaces/{ns}/images`  list images in a namespace

use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use openpanel_app::container_registry::{
    ContainerRegistryService, ImageBlob, PushError, PushRequest,
};
use openpanel_domain::container_registry::namespace::NamespaceId;
use openpanel_domain::container_registry::retention::RetentionPolicy;
use openpanel_domain::container_registry::RegistryConfig;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{ApiError, ApiResult, AuthUser};

/// Build the `/registry` routes.
pub fn router(service: Arc<ContainerRegistryService>) -> Router {
    Router::new()
        .route("/config", get(show_config).put(put_update_config))
        .route("/namespaces", post(create_namespace).get(list_namespaces))
        .route("/namespaces/{ns}/images", get(list_images))
        .route("/images", post(push_image))
        .with_state(service)
}

async fn show_config(
    State(svc): State<Arc<ContainerRegistryService>>,
    AuthUser(_user, _session): AuthUser,
) -> ApiResult<Json<RegistryConfigView>> {
    Ok(Json(RegistryConfigView::from(&svc.config())))
}

#[derive(Debug, Deserialize)]
struct UpdateConfigBody {
    retention: Option<RetentionPolicy>,
    scan_on_push: Option<bool>,
}

async fn put_update_config(
    State(svc): State<Arc<ContainerRegistryService>>,
    AuthUser(user, _session): AuthUser,
    Json(body): Json<UpdateConfigBody>,
) -> ApiResult<Json<RegistryConfigView>> {
    if !matches!(user.role(), openpanel_domain::Role::Owner | openpanel_domain::Role::Admin) {
        return Err(ApiError::Forbidden);
    }
    let mut cfg = svc.config();
    if let Some(r) = body.retention {
        cfg.retention = r;
    }
    if let Some(b) = body.scan_on_push {
        cfg.scan_on_push = b;
    }
    svc.set_config(cfg.clone());
    Ok(Json(RegistryConfigView::from(&cfg)))
}

#[derive(Debug, Deserialize)]
struct CreateNamespaceBody {
    namespace: String,
    owner: Uuid,
    quota_bytes: u64,
}

async fn create_namespace(
    State(svc): State<Arc<ContainerRegistryService>>,
    AuthUser(user, _session): AuthUser,
    Json(body): Json<CreateNamespaceBody>,
) -> ApiResult<(StatusCode, Json<NamespaceView>)> {
    if !matches!(user.role(), openpanel_domain::Role::Owner | openpanel_domain::Role::Admin) {
        return Err(ApiError::Forbidden);
    }
    let id = NamespaceId::new(&body.namespace).map_err(|e| ApiError::BadRequest(e.to_string()))?;
    let ns = svc
        .create_namespace(id, body.owner, body.quota_bytes)
        .await
        .map_err(map_push_error)?;
    Ok((StatusCode::CREATED, Json(NamespaceView::from(&ns))))
}

async fn list_namespaces(
    State(svc): State<Arc<ContainerRegistryService>>,
    AuthUser(_user, _session): AuthUser,
) -> ApiResult<Json<Vec<NamespaceView>>> {
    let list = svc.list_namespaces().await.map_err(map_push_error)?;
    Ok(Json(list.iter().map(NamespaceView::from).collect()))
}

async fn list_images(
    State(svc): State<Arc<ContainerRegistryService>>,
    AuthUser(_user, _session): AuthUser,
    Path(ns_id): Path<String>,
) -> ApiResult<Json<Vec<ImageView>>> {
    let id = NamespaceId::new(&ns_id).map_err(|e| ApiError::BadRequest(e.to_string()))?;
    let images = svc.list_images(&id).await.map_err(map_push_error)?;
    Ok(Json(images.iter().map(ImageView::from).collect()))
}

#[derive(Debug, Deserialize)]
struct PushBody {
    namespace: String,
    digest: String,
    #[serde(default)]
    reference: Option<String>,
    #[serde(default)]
    manifest: String,
    #[serde(default)]
    blobs: Vec<PushBlob>,
}

#[derive(Debug, Deserialize)]
struct PushBlob {
    digest: String,
    bytes_b64: String,
}

async fn push_image(
    State(svc): State<Arc<ContainerRegistryService>>,
    AuthUser(user, _session): AuthUser,
    Json(body): Json<PushBody>,
) -> ApiResult<(StatusCode, Json<PushResultView>)> {
    use base64::Engine;
    let id = NamespaceId::new(&body.namespace).map_err(|e| ApiError::BadRequest(e.to_string()))?;
    let manifest_digest = openpanel_domain::ImageDigest::new(&body.digest)
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;
    let mut blobs = Vec::new();
    for b in body.blobs {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(b.bytes_b64.as_bytes())
            .map_err(|_| ApiError::BadRequest("blob bytes not valid base64".into()))?;
        let digest = openpanel_domain::ImageDigest::new(&b.digest)
            .map_err(|e| ApiError::BadRequest(e.to_string()))?;
        blobs.push(ImageBlob { digest, bytes });
    }
    let req = PushRequest {
        namespace: id,
        manifest_digest,
        reference: body.reference,
        manifest_bytes: body.manifest.into_bytes(),
        blobs,
    };
    let result = svc
        .push(user.id(), req, user.username().as_str())
        .await
        .map_err(map_push_error)?;
    Ok((
        StatusCode::CREATED,
        Json(PushResultView::from(&result)),
    ))
}

fn map_push_error(e: PushError) -> ApiError {
    match e {
        PushError::Domain(d) => ApiError::BadRequest(d.to_string()),
        PushError::Persistence(msg) => ApiError::Internal(msg),
        PushError::ScanFailed(msg) => ApiError::ServiceUnavailable(msg),
        PushError::Storage(msg) => ApiError::ServiceUnavailable(msg),
    }
}

// ---- views ----

#[derive(Debug, Serialize)]
struct RegistryConfigView {
    storage_root: String,
    retention: RetentionPolicy,
    scan_on_push: bool,
}

impl From<&RegistryConfig> for RegistryConfigView {
    fn from(c: &RegistryConfig) -> Self {
        Self {
            storage_root: c.storage_root.display().to_string(),
            retention: c.retention.clone(),
            scan_on_push: c.scan_on_push,
        }
    }
}

#[derive(Debug, Serialize)]
struct NamespaceView {
    namespace_id: String,
    owner: String,
    quota_bytes: u64,
    used_bytes: u64,
    created_at: String,
}

impl From<&openpanel_domain::ImageNamespace> for NamespaceView {
    fn from(n: &openpanel_domain::ImageNamespace) -> Self {
        Self {
            namespace_id: n.namespace_id.to_string(),
            owner: n.owner.to_string(),
            quota_bytes: n.quota_bytes,
            used_bytes: n.used_bytes,
            created_at: n.created_at.to_rfc3339(),
        }
    }
}

#[derive(Debug, Serialize)]
struct ImageView {
    digest: String,
    namespace: String,
    size_bytes: u64,
    pushed_at: String,
    reference: Option<String>,
    scan_status: String,
}

impl From<&openpanel_domain::StoredImage> for ImageView {
    fn from(i: &openpanel_domain::StoredImage) -> Self {
        Self {
            digest: i.digest.to_string(),
            namespace: i.namespace.to_string(),
            size_bytes: i.size_bytes,
            pushed_at: i.pushed_at.to_rfc3339(),
            reference: i.reference.clone(),
            scan_status: i.scan_status.as_str().to_string(),
        }
    }
}

#[derive(Debug, Serialize)]
struct PushResultView {
    image: ImageView,
    scan: Option<ScanResultView>,
}

impl From<&openpanel_app::container_registry::PushResult> for PushResultView {
    fn from(r: &openpanel_app::container_registry::PushResult) -> Self {
        Self {
            image: ImageView::from(&r.image),
            scan: r.scan.as_ref().map(ScanResultView::from),
        }
    }
}

#[derive(Debug, Serialize)]
struct ScanResultView {
    digest: String,
    scanned_at: String,
    findings: Vec<ScanFindingView>,
}

impl From<&openpanel_domain::ScanResult> for ScanResultView {
    fn from(r: &openpanel_domain::ScanResult) -> Self {
        Self {
            digest: r.digest.to_string(),
            scanned_at: r.scanned_at.to_rfc3339(),
            findings: r.findings.iter().map(ScanFindingView::from).collect(),
        }
    }
}

#[derive(Debug, Serialize)]
struct ScanFindingView {
    id: String,
    severity: String,
    summary: String,
}

impl From<&openpanel_domain::ScanFinding> for ScanFindingView {
    fn from(f: &openpanel_domain::ScanFinding) -> Self {
        Self {
            id: f.id.clone(),
            severity: f.severity.clone(),
            summary: f.summary.clone(),
        }
    }
}

// Silence unused-import lint when the role guard isn't reached.
#[allow(dead_code)]
fn _ensure_into_response_marker(r: Response) -> impl IntoResponse {
    r
}