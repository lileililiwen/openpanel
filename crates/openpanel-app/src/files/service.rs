//! Files application service. RBAC + audit + canonicalize-once-per-call.

use std::sync::Arc;

use openpanel_core::{AuditAction, AuditEvent, AuditOutcome, AuditService};
use openpanel_domain::files::error::FileError;
use openpanel_domain::files::file_info::FileInfo;
use openpanel_domain::files::path::Path;
use openpanel_domain::files::repository::{FileRepository, MAX_READ_BYTES};
use openpanel_domain::sites::site::Site;
use openpanel_domain::{Role, SiteRepository, User};
use uuid::Uuid;

use crate::files::repo::FilesystemRepository;

pub struct FilesService {
    repo: Arc<dyn FileRepository>,
    sites: Arc<dyn SiteRepository>,
    audit: Arc<dyn AuditService>,
}

impl FilesService {
    pub fn new(
        sites: Arc<dyn SiteRepository>,
        audit: Arc<dyn AuditService>,
    ) -> Self {
        Self {
            repo: Arc::new(FilesystemRepository::new()),
            sites,
            audit,
        }
    }

    pub fn repo(&self) -> &Arc<dyn FileRepository> {
        &self.repo
    }

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

    pub async fn mkdir(
        &self,
        caller: &User,
        site_id: Uuid,
        path: &Path,
    ) -> Result<(), FileError> {
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

pub const MAX_BYTES: u64 = MAX_READ_BYTES;