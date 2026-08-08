//! Files application service. RBAC + audit + canonicalize-once-per-call.

use std::sync::Arc;

use openpanel_core::{AuditAction, AuditEvent, AuditOutcome, AuditService};
use openpanel_domain::{
    Role, SiteRepository, User,
    files::{
        error::FileError,
        file_info::FileInfo,
        path::Path,
        repository::{FileRepository, MAX_READ_BYTES},
    },
    sites::site::Site,
};
use uuid::Uuid;

use crate::files::repo::FilesystemRepository;

/// Application service for file operations scoped to a site document root.
pub struct FilesService {
    repo: Arc<dyn FileRepository>,
    sites: Arc<dyn SiteRepository>,
    audit: Arc<dyn AuditService>,
}

impl FilesService {
    /// Construct the service with the sites repository (for chroot lookup) and
    /// audit sink.
    pub fn new(sites: Arc<dyn SiteRepository>, audit: Arc<dyn AuditService>) -> Self {
        Self {
            repo: Arc::new(FilesystemRepository::new()),
            sites,
            audit,
        }
    }

    /// Borrow the underlying file repository (mostly for tests).
    pub fn repo(&self) -> &Arc<dyn FileRepository> {
        &self.repo
    }

    /// Return the canonicalized document root for a site the caller can manage.
    pub async fn chroot_for(
        &self,
        caller: &User,
        site_id: Uuid,
    ) -> Result<std::path::PathBuf, FileError> {
        let site = self
            .sites
            .find_by_id(site_id)
            .await
            .map_err(|e| FileError::Io(e.0))?
            .ok_or_else(|| FileError::SiteNotFound(site_id.to_string()))?;
        self.assert_can_manage(caller, &site)?;
        crate::files::repo::canonicalize_chroot(site.document_root()).await
    }

    fn assert_can_manage(&self, caller: &User, site: &Site) -> Result<(), FileError> {
        match caller.role() {
            Role::Owner | Role::Admin => Ok(()),
            Role::User => {
                if site.owner_id() == caller.id() {
                    Ok(())
                } else {
                    Err(FileError::Forbidden)
                }
            }
        }
    }

    async fn load_site(&self, caller: &User, site_id: Uuid) -> Result<Site, FileError> {
        let site = self
            .sites
            .find_by_id(site_id)
            .await
            .map_err(|e| FileError::Io(e.0))?
            .ok_or_else(|| FileError::SiteNotFound(site_id.to_string()))?;
        self.assert_can_manage(caller, &site)?;
        Ok(site)
    }

    /// List the entries in a site-relative directory.
    pub async fn list_dir(
        &self,
        caller: &User,
        site_id: Uuid,
        path: &Path,
    ) -> Result<Vec<FileInfo>, FileError> {
        let site = self.load_site(caller, site_id).await?;
        let chroot = crate::files::repo::canonicalize_chroot(site.document_root()).await?;
        self.repo.list_dir(&chroot, path).await
    }

    /// Read a file's bytes plus its modification time.
    pub async fn read_file(
        &self,
        caller: &User,
        site_id: Uuid,
        path: &Path,
    ) -> Result<(Vec<u8>, chrono::DateTime<chrono::Utc>), FileError> {
        let site = self.load_site(caller, site_id).await?;
        let chroot = crate::files::repo::canonicalize_chroot(site.document_root()).await?;
        self.repo.read_file(&chroot, path).await
    }

    /// Write `bytes` to a site-relative path, creating parents if needed.
    pub async fn write_file(
        &self,
        caller: &User,
        site_id: Uuid,
        path: &Path,
        bytes: &[u8],
    ) -> Result<(), FileError> {
        let site = self.load_site(caller, site_id).await?;
        let chroot = crate::files::repo::canonicalize_chroot(site.document_root()).await?;
        self.repo.write_file(&chroot, path, bytes).await?;
        self.audit
            .record(
                AuditEvent::new(
                    caller.username().as_str(),
                    AuditAction::FileUpdated,
                    AuditOutcome::Success,
                )
                .target(site_id.to_string())
                .metadata(serde_json::json!({
                    "path": path.to_string(),
                    "bytes": bytes.len(),
                })),
            )
            .await
            .ok();
        Ok(())
    }

    /// Create a directory at the given site-relative path.
    pub async fn mkdir(&self, caller: &User, site_id: Uuid, path: &Path) -> Result<(), FileError> {
        let site = self.load_site(caller, site_id).await?;
        let chroot = crate::files::repo::canonicalize_chroot(site.document_root()).await?;
        self.repo.mkdir(&chroot, path).await?;
        self.audit
            .record(
                AuditEvent::new(
                    caller.username().as_str(),
                    AuditAction::FileUploaded,
                    AuditOutcome::Success,
                )
                .target(site_id.to_string())
                .metadata(serde_json::json!({
                    "path": path.to_string(),
                    "kind": "directory",
                })),
            )
            .await
            .ok();
        Ok(())
    }

    /// Remove a file or directory (`recursive=true` to wipe non-empty dirs).
    pub async fn remove(
        &self,
        caller: &User,
        site_id: Uuid,
        path: &Path,
        recursive: bool,
    ) -> Result<(), FileError> {
        let site = self.load_site(caller, site_id).await?;
        let chroot = crate::files::repo::canonicalize_chroot(site.document_root()).await?;
        self.repo.remove(&chroot, path, recursive).await?;
        self.audit
            .record(
                AuditEvent::new(
                    caller.username().as_str(),
                    AuditAction::FileDeleted,
                    AuditOutcome::Success,
                )
                .target(site_id.to_string())
                .metadata(serde_json::json!({
                    "path": path.to_string(),
                    "recursive": recursive,
                })),
            )
            .await
            .ok();
        Ok(())
    }

    /// Rename or move a file/directory within the same site chroot.
    pub async fn rename(
        &self,
        caller: &User,
        site_id: Uuid,
        from: &Path,
        to: &Path,
    ) -> Result<(), FileError> {
        let site = self.load_site(caller, site_id).await?;
        let chroot = crate::files::repo::canonicalize_chroot(site.document_root()).await?;
        self.repo.rename(&chroot, from, to).await?;
        self.audit
            .record(
                AuditEvent::new(
                    caller.username().as_str(),
                    AuditAction::FileRenamed,
                    AuditOutcome::Success,
                )
                .target(site_id.to_string())
                .metadata(serde_json::json!({
                    "from": from.to_string(),
                    "to": to.to_string(),
                })),
            )
            .await
            .ok();
        Ok(())
    }

    /// Set the POSIX mode bits on a file or directory.
    pub async fn chmod(
        &self,
        caller: &User,
        site_id: Uuid,
        path: &Path,
        mode: u32,
    ) -> Result<(), FileError> {
        let site = self.load_site(caller, site_id).await?;
        let chroot = crate::files::repo::canonicalize_chroot(site.document_root()).await?;
        self.repo.chmod(&chroot, path, mode).await?;
        self.audit
            .record(
                AuditEvent::new(
                    caller.username().as_str(),
                    AuditAction::FileModeChanged,
                    AuditOutcome::Success,
                )
                .target(site_id.to_string())
                .metadata(serde_json::json!({
                    "path": path.to_string(),
                    "mode": format!("{mode:04o}"),
                })),
            )
            .await
            .ok();
        Ok(())
    }
}

/// Maximum bytes the files service will read from a file in a single call.
pub const MAX_BYTES: u64 = MAX_READ_BYTES;
