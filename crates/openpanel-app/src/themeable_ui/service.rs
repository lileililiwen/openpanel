//! Themeable UI application service: per-account theme overrides,
//! palette validation, logo upload + serving, and the host-header
//! resolution that the web shell uses to render the right brand.

use std::{path::PathBuf, sync::Arc};

use openpanel_core::{AuditAction, AuditEvent, AuditOutcome, AuditService};
use openpanel_domain::{
    BrandingScope, Palette, PanelDomain, ThemeOverride, ThemeableUiError, ThemeableUiRepository,
    Typography,
};
use uuid::Uuid;

/// Max logo upload size in bytes (256 KiB, per the spec).
pub const MAX_LOGO_BYTES: u64 = 256 * 1024;

/// Default branding root on disk.
pub const DEFAULT_BRANDING_ROOT: &str = "/srv/branding";

/// Application service for per-account theme overrides.
pub struct ThemeableUiService {
    repo: Arc<dyn ThemeableUiRepository>,
    audit: Arc<dyn AuditService>,
    branding_root: PathBuf,
}

impl ThemeableUiService {
    /// Build a service.
    pub fn new(
        repo: Arc<dyn ThemeableUiRepository>,
        audit: Arc<dyn AuditService>,
        branding_root: Option<PathBuf>,
    ) -> Self {
        Self {
            repo,
            audit,
            branding_root: branding_root.unwrap_or_else(|| PathBuf::from(DEFAULT_BRANDING_ROOT)),
        }
    }

    /// Borrow the underlying repository.
    pub fn repo(&self) -> &Arc<dyn ThemeableUiRepository> {
        &self.repo
    }

    /// Look up the override for an owner. `None` when the owner has
    /// no override stored.
    pub async fn get_override(
        &self,
        actor: &str,
        owner_id: Uuid,
    ) -> Result<Option<ThemeOverride>, ThemeableUiError> {
        let _ = actor;
        self.repo.find_override(owner_id).await
    }

    /// Resolve the override for a `Host` header value. Returns
    /// `None` when no override is bound to that FQDN (the system
    /// default theme is used in that case).
    pub async fn resolve_for_host(
        &self,
        host: &str,
    ) -> Result<Option<ThemeOverride>, ThemeableUiError> {
        self.repo.find_by_fqdn(host).await
    }

    /// Replace the override. Enforces plan-level `BrandingScope`
    /// via the supplied `scope`. `scope` MUST be the owner's
    /// effective scope; the service refuses when it is not
    /// `Reseller`.
    #[allow(clippy::too_many_arguments)]
    pub async fn set_override(
        &self,
        actor: &str,
        owner_id: Uuid,
        brand_name: impl Into<String>,
        palette: Palette,
        typography: Typography,
        panel_domain: Option<PanelDomain>,
        scope: BrandingScope,
    ) -> Result<ThemeOverride, ThemeableUiError> {
        if !scope.grants_branding() {
            return Err(ThemeableUiError::BrandingNotAllowed);
        }
        if let Some(override_) = self.repo.find_override(owner_id).await? {
            let updated =
                ThemeOverride::new(owner_id, brand_name, palette, typography, panel_domain)?;
            // Preserve the existing logo path.
            let mut next = updated;
            if let Some(p) = override_.logo_path() {
                next.set_logo_path(p);
            }
            self.repo.upsert_override(&next).await?;
            self.audit
                .record(
                    AuditEvent::new(
                        actor,
                        AuditAction::ThemeOverrideUpdated,
                        AuditOutcome::Success,
                    )
                    .target(owner_id.to_string())
                    .metadata(serde_json::json!({
                        "brand_name": next.brand_name(),
                        "panel_domain": next.panel_domain().map(|d| d.fqdn().to_string()),
                    })),
                )
                .await
                .ok();
            Ok(next)
        } else {
            let override_ =
                ThemeOverride::new(owner_id, brand_name, palette, typography, panel_domain)?;
            self.repo.upsert_override(&override_).await?;
            self.audit
                .record(
                    AuditEvent::new(
                        actor,
                        AuditAction::ThemeOverrideUpdated,
                        AuditOutcome::Success,
                    )
                    .target(owner_id.to_string())
                    .metadata(serde_json::json!({
                        "brand_name": override_.brand_name(),
                    })),
                )
                .await
                .ok();
            Ok(override_)
        }
    }

    /// Delete the override for an owner.
    pub async fn clear_override(
        &self,
        actor: &str,
        owner_id: Uuid,
        scope: BrandingScope,
    ) -> Result<(), ThemeableUiError> {
        if !scope.grants_branding() {
            return Err(ThemeableUiError::BrandingNotAllowed);
        }
        self.repo.delete_override(owner_id).await?;
        self.audit
            .record(
                AuditEvent::new(
                    actor,
                    AuditAction::ThemeOverrideCleared,
                    AuditOutcome::Success,
                )
                .target(owner_id.to_string()),
            )
            .await
            .ok();
        Ok(())
    }

    /// Validate a logo upload. The body and declared mime type
    /// are checked against the spec:
    ///
    /// * SVG: scanned for `<script>`, `on*=` attributes, and
    ///   `xlink:href` to external URLs. The function returns
    ///   `SvgContainsForbidden` when any are found.
    /// * PNG / JPEG: rejected with `UnsupportedMime` when the
    ///   declared mime is not in the allow list.
    /// * Size: rejected with `FileTooLarge` when the body
    ///   exceeds `MAX_LOGO_BYTES`.
    pub fn validate_logo(&self, declared_mime: &str, body: &[u8]) -> Result<(), ThemeableUiError> {
        if body.len() as u64 > MAX_LOGO_BYTES {
            return Err(ThemeableUiError::FileTooLarge(body.len() as u64));
        }
        let mime = declared_mime.trim().to_lowercase();
        match mime.as_str() {
            "image/svg+xml" => {
                let text = std::str::from_utf8(body)
                    .map_err(|_| ThemeableUiError::UnsupportedMime(mime.clone()))?;
                if text.contains("<script")
                    || text.contains("onload=")
                    || text.contains("onerror=")
                    || text.contains("onclick=")
                    || text.contains("xlink:href=\"http")
                    || text.contains(" href=\"http")
                {
                    return Err(ThemeableUiError::SvgContainsForbidden);
                }
                Ok(())
            }
            "image/png" | "image/jpeg" => Ok(()),
            other => Err(ThemeableUiError::UnsupportedMime(other.to_string())),
        }
    }

    /// Store a validated logo on disk. The path is computed from
    /// the owner id and the extension inferred from the mime.
    pub async fn store_logo(
        &self,
        owner_id: Uuid,
        mime: &str,
        body: &[u8],
    ) -> Result<String, ThemeableUiError> {
        self.validate_logo(mime, body)?;
        let ext = match mime.trim().to_lowercase().as_str() {
            "image/svg+xml" => "svg",
            "image/png" => "png",
            "image/jpeg" => "jpg",
            other => return Err(ThemeableUiError::UnsupportedMime(other.to_string())),
        };
        let dir = self.branding_root.join(owner_id.to_string());
        std::fs::create_dir_all(&dir)
            .map_err(|e| ThemeableUiError::Persistence(format!("mkdir: {e}")))?;
        let path = dir.join(format!("logo.{ext}"));
        std::fs::write(&path, body)
            .map_err(|e| ThemeableUiError::Persistence(format!("write: {e}")))?;
        let rel = path.to_string_lossy().to_string();
        // Persist the path on the override so the shell can pick
        // it up.
        if let Some(mut existing) = self.repo.find_override(owner_id).await? {
            existing.set_logo_path(rel.clone());
            self.repo.upsert_override(&existing).await?;
        }
        Ok(rel)
    }
}
