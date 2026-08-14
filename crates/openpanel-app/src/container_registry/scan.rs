//! Scan hook abstraction and a default no-op implementation.

use async_trait::async_trait;
use openpanel_domain::container_registry::image::ImageDigest;
use openpanel_domain::container_registry::scan::ScanResult;
use std::sync::Arc;

/// Errors raised by a scan hook.
#[derive(Debug, thiserror::Error)]
pub enum ScanHookError {
    /// The hook itself failed (scanner unavailable, etc).
    #[error("scan failed: {0}")]
    Failed(String),
}

/// Pluggable vulnerability-scan hook invoked after each push.
#[async_trait]
pub trait ScanHook: Send + Sync {
    /// Scan an image by digest. The hook MUST NOT auto-delete
    /// images; findings are recorded but the registry keeps the
    /// image in place.
    async fn scan(&self, digest: &ImageDigest) -> Result<ScanResult, ScanHookError>;
}

/// Default no-op scan hook (returns `Clean`). Tests wire this; the
/// production follow-on change will replace it with a Trivy /
/// Grype adapter.
#[derive(Default, Clone)]
pub struct NoopScanHook;

#[async_trait]
impl ScanHook for NoopScanHook {
    async fn scan(&self, digest: &ImageDigest) -> Result<ScanResult, ScanHookError> {
        Ok(ScanResult::new(
            digest.clone(),
            chrono::Utc::now(),
            Vec::new(),
        ))
    }
}

/// Wrap an arbitrary closure as a scan hook. Useful in tests.
pub struct FnScanHook(pub Arc<dyn Fn(&ImageDigest) -> Result<ScanResult, ScanHookError> + Send + Sync>);

#[async_trait]
impl ScanHook for FnScanHook {
    async fn scan(&self, digest: &ImageDigest) -> Result<ScanResult, ScanHookError> {
        (self.0)(digest)
    }
}