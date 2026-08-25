//! Site HTTP-controls application use cases.

use std::{path::PathBuf, sync::Arc};

use async_trait::async_trait;
use openpanel_core::{AuditAction, AuditEvent, AuditOutcome, AuditService};
use openpanel_domain::{
    Role, Site, SiteRepository, User,
    site_http_controls::{SiteHttpControls, SiteHttpError, SiteHttpRepository},
};

use super::SiteHttpControlsRenderer;

/// Port that atomically tests and activates a compiled site snippet.
#[async_trait]
pub trait SiteHttpConfigApplier: Send + Sync + 'static {
    /// Apply the snippet to the site's rendered nginx candidate.
    async fn apply(&self, site: &Site, snippet: &str) -> Result<(), SiteHttpError>;
}

/// Production applier backed by the sites nginx generator.
pub struct NginxHttpControlsApplier {
    generator: crate::sites::nginx::NginxConfigGenerator,
}

impl NginxHttpControlsApplier {
    /// Construct an applier over the shared nginx paths.
    pub fn new(generator: crate::sites::nginx::NginxConfigGenerator) -> Self {
        Self { generator }
    }
}

#[async_trait]
impl SiteHttpConfigApplier for NginxHttpControlsApplier {
    async fn apply(&self, site: &Site, snippet: &str) -> Result<(), SiteHttpError> {
        // `apply_with_waf` is the generic insert-before-first-location
        // pipeline (atomic write, `nginx -t`, rollback, reload).
        self.generator
            .apply_with_waf(site, snippet)
            .map_err(|error| SiteHttpError::Persistence(bounded(&error.to_string())))
    }
}

/// Owner-only per-site HTTP-controls orchestration service.
pub struct SiteHttpService {
    repo: Arc<dyn SiteHttpRepository>,
    sites: Arc<dyn SiteRepository>,
    audit: Arc<dyn AuditService>,
    applier: Arc<dyn SiteHttpConfigApplier>,
    auth_dir: PathBuf,
}

impl SiteHttpService {
    /// Construct with persistence, site lookup, audit, config ports,
    /// and the directory that receives htpasswd files.
    pub fn new(
        repo: Arc<dyn SiteHttpRepository>,
        sites: Arc<dyn SiteRepository>,
        audit: Arc<dyn AuditService>,
        applier: Arc<dyn SiteHttpConfigApplier>,
        auth_dir: PathBuf,
    ) -> Self {
        Self {
            repo,
            sites,
            audit,
            applier,
            auth_dir,
        }
    }

    /// Get a site's controls document, returning an empty document when absent.
    pub async fn get(
        &self,
        caller: &User,
        site_id: Uuid,
    ) -> Result<SiteHttpControls, SiteHttpError> {
        owner(caller)?;
        self.require_site(site_id).await?;
        Ok(self
            .repo
            .get(site_id)
            .await
            .map_err(persistence)?
            .unwrap_or_else(|| SiteHttpControls::empty(site_id)))
    }

    /// Atomically replace a complete controls document after nginx validation.
    pub async fn put(
        &self,
        caller: &User,
        desired: SiteHttpControls,
    ) -> Result<SiteHttpControls, SiteHttpError> {
        owner(caller)?;
        let desired = desired.validated()?;
        let site = self.require_site(desired.site_id()).await?;
        let current = self
            .repo
            .get(desired.site_id())
            .await
            .map_err(persistence)?;
        if current.as_ref() == Some(&desired) {
            return Ok(desired);
        }
        let htpasswd_prefix = self.htpasswd_prefix(desired.site_id());
        let snippet = SiteHttpControlsRenderer::compile(&desired, &htpasswd_prefix)?;
        if !desired.protected_dirs().is_empty() {
            self.write_htpasswd_files(desired.site_id(), desired.protected_dirs())?;
        }
        if let Err(error) = self.applier.apply(&site, &snippet).await {
            let _ = self
                .audit
                .record(
                    AuditEvent::new(
                        caller.username().as_str(),
                        AuditAction::SiteHttpControlsChanged,
                        AuditOutcome::Failure,
                    )
                    .target(desired.site_id().to_string())
                    .metadata(serde_json::json!({"reason": bounded(&error.to_string())})),
                )
                .await;
            return Err(error);
        }
        self.repo.put(&desired).await.map_err(persistence)?;
        let _ = self
            .audit
            .record(
                AuditEvent::new(
                    caller.username().as_str(),
                    AuditAction::SiteHttpControlsChanged,
                    AuditOutcome::Success,
                )
                .target(desired.site_id().to_string())
                .metadata(serde_json::json!({
                    "version": desired.version(),
                    "error_pages": desired.error_pages().len(),
                    "redirects": desired.redirects().len(),
                    "protected_dirs": desired.protected_dirs().len(),
                    "ip_rules": desired.ip_rules().len(),
                    "mime_overrides": desired.mime_overrides().len(),
                })),
            )
            .await;
        Ok(desired)
    }

    fn htpasswd_prefix(&self, site_id: Uuid) -> String {
        self.auth_dir
            .join(site_id.to_string())
            .to_string_lossy()
            .into_owned()
    }

    /// Write one htpasswd file per protected directory. Files contain
    /// bcrypt hashes only and are written with mode 0600.
    fn write_htpasswd_files(
        &self,
        site_id: Uuid,
        dirs: &[openpanel_domain::site_http_controls::ProtectedDir],
    ) -> Result<(), SiteHttpError> {
        use std::{fs, io::Write, os::unix::fs::PermissionsExt};

        let dir = self.auth_dir.join(site_id.to_string());
        fs::create_dir_all(&dir)
            .map_err(|error| SiteHttpError::Persistence(bounded(&error.to_string())))?;
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o700))
            .map_err(|error| SiteHttpError::Persistence(bounded(&error.to_string())))?;
        for (index, protected) in dirs.iter().enumerate() {
            let path = dir.join(format!("{index}.htpasswd"));
            let mut file = fs::File::create(&path)
                .map_err(|error| SiteHttpError::Persistence(bounded(&error.to_string())))?;
            for account in &protected.accounts {
                writeln!(file, "{}:{}", account.name, account.password_hash)
                    .map_err(|error| SiteHttpError::Persistence(bounded(&error.to_string())))?;
            }
            file.set_permissions(fs::Permissions::from_mode(0o600))
                .map_err(|error| SiteHttpError::Persistence(bounded(&error.to_string())))?;
        }
        Ok(())
    }

    async fn require_site(&self, site_id: Uuid) -> Result<Site, SiteHttpError> {
        self.sites
            .find_by_id(site_id)
            .await
            .map_err(persistence)?
            .ok_or_else(|| SiteHttpError::SiteNotFound(site_id.to_string()))
    }
}

use uuid::Uuid;

fn owner(caller: &User) -> Result<(), SiteHttpError> {
    if caller.role() != Role::Owner {
        return Err(SiteHttpError::Forbidden);
    }
    Ok(())
}

fn persistence(error: impl std::fmt::Display) -> SiteHttpError {
    SiteHttpError::Persistence(bounded(&error.to_string()))
}

fn bounded(value: &str) -> String {
    value.chars().take(512).collect()
}
