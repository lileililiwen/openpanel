//! `StagingSlotFilesystemLayer` trait: filesystem operations the
//! staging service delegates to. Production code wires
//! shell-outs to `rsync` and `nginx`; tests wire an in-memory fake.

use async_trait::async_trait;
use chrono::{DateTime, Utc};

use crate::site_staging::SiteStagingError;

/// Outcome of an atomic promote: either the new docroot is live
/// (`Success`) or a rollback was performed (`RolledBack`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromoteOutcome {
    /// The rename chain completed and nginx reloaded successfully.
    Success,
    /// A step failed; the rollback chain was performed. The
    /// `reason` field is a sanitized error message.
    RolledBack {
        /// Sanitized error message from the failed step.
        reason: String,
    },
}

/// Filesystem abstraction for the staging bounded context.
///
/// All methods are async because production code shells out to
/// `rsync`, `mysqldump`, and `nginx -s reload`. Tests use the
/// in-memory implementation in [`crate::site_staging::InMemoryStagingFilesystem`].
#[async_trait]
pub trait StagingFilesystemLayer: Send + Sync + 'static {
    /// Create the staging docroot and any snapshot directories.
    async fn create_slot(
        &self,
        site_id: uuid::Uuid,
        document_root: &str,
    ) -> Result<(), SiteStagingError>;

    /// Copy the production docroot into the staging docroot, then
    /// dump the production database into the staging database.
    /// Returns the wall-clock instant the snapshot was taken.
    async fn sync_snapshot(
        &self,
        site_id: uuid::Uuid,
        prod_docroot: &str,
        staging_docroot: &str,
    ) -> Result<DateTime<Utc>, SiteStagingError>;

    /// Atomic promote: rename chain + nginx reload. On any failure
    /// the rename chain is reversed and nginx is reloaded again.
    async fn promote(
        &self,
        site_id: uuid::Uuid,
        prod_docroot: &str,
        staging_docroot: &str,
    ) -> Result<PromoteOutcome, SiteStagingError>;

    /// Remove the staging docroot and drop the staging database.
    async fn destroy_slot(
        &self,
        site_id: uuid::Uuid,
        document_root: &str,
        db_name: &str,
    ) -> Result<(), SiteStagingError>;
}

/// In-memory implementation of `StagingFilesystemLayer` for tests.
pub struct InMemoryStagingFilesystem {
    inner: std::sync::Arc<tokio::sync::Mutex<InMemoryStagingFilesystemState>>,
    /// Force `promote` to return `RolledBack`. Used by tests that
    /// exercise the rollback path.
    force_rollback: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

struct InMemoryStagingFilesystemState {
    /// site_id -> set of paths known to exist
    paths: std::collections::HashMap<uuid::Uuid, std::collections::HashSet<String>>,
    /// promote call count, used by tests to verify
    /// `force_rollback` actually fires.
    promote_calls: u32,
}

impl InMemoryStagingFilesystem {
    /// Build a new in-memory filesystem layer.
    pub fn new() -> Self {
        Self {
            inner: std::sync::Arc::new(tokio::sync::Mutex::new(InMemoryStagingFilesystemState {
                paths: std::collections::HashMap::new(),
                promote_calls: 0,
            })),
            force_rollback: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    /// Force the next `promote` call to return `RolledBack`.
    pub fn force_next_promote_rollback(&self) {
        self.force_rollback
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }

    /// Number of times `promote` has been called on this filesystem.
    pub async fn promote_calls(&self) -> u32 {
        self.inner.lock().await.promote_calls
    }
}

impl Default for InMemoryStagingFilesystem {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl StagingFilesystemLayer for InMemoryStagingFilesystem {
    async fn create_slot(
        &self,
        site_id: uuid::Uuid,
        document_root: &str,
    ) -> Result<(), SiteStagingError> {
        let mut g = self.inner.lock().await;
        g.paths
            .entry(site_id)
            .or_default()
            .insert(document_root.to_string());
        Ok(())
    }

    async fn sync_snapshot(
        &self,
        site_id: uuid::Uuid,
        prod_docroot: &str,
        staging_docroot: &str,
    ) -> Result<DateTime<Utc>, SiteStagingError> {
        let mut g = self.inner.lock().await;
        let entry = g.paths.entry(site_id).or_default();
        entry.insert(prod_docroot.to_string());
        entry.insert(staging_docroot.to_string());
        Ok(Utc::now())
    }

    async fn promote(
        &self,
        site_id: uuid::Uuid,
        prod_docroot: &str,
        staging_docroot: &str,
    ) -> Result<PromoteOutcome, SiteStagingError> {
        let mut g = self.inner.lock().await;
        g.promote_calls += 1;
        let entry = g.paths.entry(site_id).or_default();
        entry.insert(prod_docroot.to_string());
        entry.insert(staging_docroot.to_string());
        if self
            .force_rollback
            .swap(false, std::sync::atomic::Ordering::SeqCst)
        {
            Ok(PromoteOutcome::RolledBack {
                reason: "forced".into(),
            })
        } else {
            Ok(PromoteOutcome::Success)
        }
    }

    async fn destroy_slot(
        &self,
        site_id: uuid::Uuid,
        document_root: &str,
        _db_name: &str,
    ) -> Result<(), SiteStagingError> {
        let mut g = self.inner.lock().await;
        if let Some(paths) = g.paths.get_mut(&site_id) {
            paths.remove(document_root);
        }
        Ok(())
    }
}
