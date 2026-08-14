//! Container registry service: namespaces, push, scan, retention.

use std::sync::Arc;

use chrono::Utc;
use openpanel_core::audit::{AuditAction, AuditEvent, AuditOutcome, AuditService};
use openpanel_domain::common::error::RepoError;
use openpanel_domain::container_registry::image::{ImageDigest, ScanStatus, StoredImage};
use openpanel_domain::container_registry::namespace::{ImageNamespace, NamespaceId};
use openpanel_domain::container_registry::retention::RetentionPolicy;
use openpanel_domain::container_registry::scan::ScanResult;
use openpanel_domain::container_registry::RegistryConfig;
use openpanel_domain::RegistryError;
use uuid::Uuid;

use super::repo::{ImageRepository, NamespaceRepository, ScanResultRepository};
use super::scan::ScanHook;
use super::storage::StorageLayer;

/// A single image blob uploaded during a push.
#[derive(Debug, Clone)]
pub struct ImageBlob {
    /// OCI blob digest (`sha256:...`).
    pub digest: ImageDigest,
    /// Bytes.
    pub bytes: Vec<u8>,
}

/// Push request.
#[derive(Debug, Clone)]
pub struct PushRequest {
    /// Target namespace.
    pub namespace: NamespaceId,
    /// Image manifest digest.
    pub manifest_digest: ImageDigest,
    /// Reference (tag), optional.
    pub reference: Option<String>,
    /// Manifest bytes.
    pub manifest_bytes: Vec<u8>,
    /// Blobs uploaded alongside the manifest.
    pub blobs: Vec<ImageBlob>,
}

/// Push result.
#[derive(Debug, Clone)]
pub struct PushResult {
    /// Stored image record.
    pub image: StoredImage,
    /// Optional scan result (present when `scan_on_push` was set).
    pub scan: Option<ScanResult>,
}

/// Errors raised by push / scan / retention flows.
#[derive(Debug, thiserror::Error)]
pub enum PushError {
    /// Domain / validation failure.
    #[error(transparent)]
    Domain(#[from] RegistryError),
    /// Persistence failure.
    #[error("persistence error: {0}")]
    Persistence(String),
    /// Scan hook failed (the image itself is still stored).
    #[error("scan hook failed: {0}")]
    ScanFailed(String),
    /// Storage layer failed.
    #[error("storage error: {0}")]
    Storage(String),
}

impl From<RepoError> for PushError {
    fn from(e: RepoError) -> Self {
        Self::Persistence(e.0)
    }
}

impl From<super::storage::StorageError> for PushError {
    fn from(e: super::storage::StorageError) -> Self {
        Self::Storage(e.to_string())
    }
}

/// Container registry service.
pub struct ContainerRegistryService {
    namespaces: Arc<dyn NamespaceRepository>,
    images: Arc<dyn ImageRepository>,
    scans: Arc<dyn ScanResultRepository>,
    storage: Arc<dyn StorageLayer>,
    scan_hook: Arc<dyn ScanHook>,
    audit: Arc<dyn AuditService>,
    config: std::sync::RwLock<RegistryConfig>,
}

impl ContainerRegistryService {
    /// Construct a new service.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        namespaces: Arc<dyn NamespaceRepository>,
        images: Arc<dyn ImageRepository>,
        scans: Arc<dyn ScanResultRepository>,
        storage: Arc<dyn StorageLayer>,
        scan_hook: Arc<dyn ScanHook>,
        audit: Arc<dyn AuditService>,
        config: RegistryConfig,
    ) -> Self {
        Self {
            namespaces,
            images,
            scans,
            storage,
            scan_hook,
            audit,
            config: std::sync::RwLock::new(config),
        }
    }

    /// Replace the registry configuration.
    pub fn set_config(&self, config: RegistryConfig) {
        *self.config.write().unwrap() = config;
    }

    /// Snapshot the current registry configuration.
    pub fn config(&self) -> RegistryConfig {
        self.config.read().unwrap().clone()
    }

    /// Create a new namespace.
    pub async fn create_namespace(
        &self,
        namespace_id: NamespaceId,
        owner: Uuid,
        quota_bytes: u64,
    ) -> Result<ImageNamespace, PushError> {
        let ns = ImageNamespace::new(namespace_id, owner, quota_bytes, Utc::now());
        self.namespaces.insert(&ns).await?;
        Ok(ns)
    }

    /// Look up a namespace.
    pub async fn find_namespace(
        &self,
        id: &NamespaceId,
    ) -> Result<Option<ImageNamespace>, PushError> {
        Ok(self.namespaces.find(id).await?)
    }

    /// List namespaces.
    pub async fn list_namespaces(&self) -> Result<Vec<ImageNamespace>, PushError> {
        Ok(self.namespaces.list().await?)
    }

    /// Look up an image.
    pub async fn find_image(
        &self,
        namespace: &NamespaceId,
        digest: &ImageDigest,
    ) -> Result<Option<StoredImage>, PushError> {
        Ok(self.images.find(namespace, digest).await?)
    }

    /// List images in a namespace.
    pub async fn list_images(
        &self,
        namespace: &NamespaceId,
    ) -> Result<Vec<StoredImage>, PushError> {
        Ok(self.images.list_for_namespace(namespace).await?)
    }

    /// Latest scan result for an image (if any).
    pub async fn latest_scan(
        &self,
        digest: &ImageDigest,
    ) -> Result<Option<ScanResult>, PushError> {
        Ok(self.scans.latest_for(digest).await?)
    }

    /// Push an image: authz, write blobs + manifest, optionally scan,
    /// apply retention. The push returns the persisted record and
    /// the scan result (if any). Cross-namespace pushes are refused.
    pub async fn push(
        &self,
        caller: Uuid,
        request: PushRequest,
        actor: &str,
    ) -> Result<PushResult, PushError> {
        let ns = self
            .namespaces
            .find(&request.namespace)
            .await?
            .ok_or_else(|| {
                PushError::Domain(RegistryError::NamespaceNotFound(
                    request.namespace.to_string(),
                ))
            })?;
        if ns.owner != caller {
            return Err(PushError::Domain(RegistryError::NotAuthorised));
        }
        let total_bytes = request
            .blobs
            .iter()
            .map(|b| b.bytes.len() as u64)
            .sum::<u64>()
            + request.manifest_bytes.len() as u64;
        if ns.would_exceed(total_bytes) {
            return Err(PushError::Domain(RegistryError::QuotaExceeded {
                used: ns.used_bytes,
                delta: total_bytes,
                limit: ns.quota_bytes,
            }));
        }
        // Persist blobs at storage_root/<ns>/blobs/<digest>.
        for blob in &request.blobs {
            let path = format!(
                "{}/blobs/{}",
                ns.namespace_id.as_str(),
                blob.digest.as_str()
            );
            self.storage.write(&path, &blob.bytes).await?;
        }
        let manifest_path = format!(
            "{}/manifests/{}",
            ns.namespace_id.as_str(),
            request.manifest_digest.as_str()
        );
        self.storage.write(&manifest_path, &request.manifest_bytes).await?;
        // Persist the image record.
        let mut image = StoredImage::new_pushed(
            request.manifest_digest.clone(),
            ns.namespace_id.clone(),
            total_bytes,
            request.reference.clone(),
            Utc::now(),
        );
        self.images.insert(&image).await?;
        // Account the storage against the namespace.
        let mut updated_ns = ns.clone();
        updated_ns.account_push(total_bytes);
        self.namespaces
            .update_used(&updated_ns.namespace_id, updated_ns.used_bytes)
            .await?;
        // Run the scan hook if configured.
        let mut scan_result: Option<ScanResult> = None;
        let cfg = self.config();
        if cfg.scan_on_push {
            match self.scan_hook.scan(&image.digest).await {
                Ok(result) => {
                    let status = if result.has_findings() {
                        ScanStatus::WithFindings
                    } else {
                        ScanStatus::Clean
                    };
                    image.scan_status = status;
                    self.images
                        .update_status(&image.namespace, &image.digest, status)
                        .await?;
                    self.scans.insert(&result).await?;
                    scan_result = Some(result);
                }
                Err(_err) => {
                    // The image remains; the registry records the
                    // hook failure but does not auto-delete.
                    image.scan_status = ScanStatus::Failed;
                    self.images
                        .update_status(&image.namespace, &image.digest, ScanStatus::Failed)
                        .await?;
                    return Err(PushError::ScanFailed("scanner unavailable".into()));
                }
            }
        }
        // Apply retention.
        self.apply_retention(&image.namespace, &cfg.retention, actor).await?;
        let event = AuditEvent::new(
            actor,
            AuditAction::RegistryImagePushed,
            AuditOutcome::Success,
        )
        .metadata(serde_json::json!({
            "namespace": image.namespace.as_str(),
            "digest": image.digest.as_str(),
        }));
        self.audit.record(event).await.ok();
        Ok(PushResult {
            image,
            scan: scan_result,
        })
    }

    /// Apply the retention policy to a namespace. Prunes oldest
    /// over-count or over-age images, removes their blobs, and
    /// updates `used_bytes`.
    pub async fn apply_retention(
        &self,
        namespace: &NamespaceId,
        policy: &RetentionPolicy,
        actor: &str,
    ) -> Result<u32, PushError> {
        let ns = self
            .namespaces
            .find(namespace)
            .await?
            .ok_or_else(|| {
                PushError::Domain(RegistryError::NamespaceNotFound(namespace.to_string()))
            })?;
        let images = self.images.list_for_namespace(namespace).await?;
        let entries = images
            .iter()
            .map(|i| (i.digest.as_str().to_string(), i.pushed_at));
        let verdict = policy.apply(entries, Utc::now());
        let mut pruned = 0u32;
        for digest_str in &verdict.to_delete {
            let digest = ImageDigest::new(digest_str.clone())
                .map_err(|e| PushError::Domain(e))?;
            if let Some(image) = self.images.find(namespace, &digest).await? {
                let path = format!(
                    "{}/manifests/{}",
                    namespace.as_str(),
                    digest.as_str()
                );
                self.storage.delete(&path).await.ok();
                self.images.delete(namespace, &digest).await?;
                let mut updated = ns.clone();
                updated.account_delete(image.size_bytes);
                self.namespaces
                    .update_used(namespace, updated.used_bytes)
                    .await?;
                pruned += 1;
                let _ = actor;
            }
        }
        Ok(pruned)
    }
}