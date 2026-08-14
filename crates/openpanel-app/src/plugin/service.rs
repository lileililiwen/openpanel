//! Plugin install / lifecycle service.

use chrono::Utc;
use openpanel_core::audit::{AuditAction, AuditEvent, AuditOutcome, AuditService};
use openpanel_domain::{
    PluginError, PluginId, PluginManifest, PluginRegistry, PluginStatus,
};
use std::sync::Arc;

/// Errors raised by the plugin lifecycle service.
#[derive(Debug, thiserror::Error)]
pub enum PluginInstallError {
    /// The manifest is invalid.
    #[error(transparent)]
    Invalid(#[from] PluginError),
    /// Persistence failed.
    #[error("persistence error: {0}")]
    Persistence(String),
}

impl From<openpanel_domain::common::error::RepoError> for PluginInstallError {
    fn from(e: openpanel_domain::common::error::RepoError) -> Self {
        Self::Persistence(e.0)
    }
}

/// Plugin lifecycle service. Installs a verified manifest into the
/// `PluginRegistry` and emits the audit events.
pub struct PluginService {
    registry: Arc<dyn PluginRegistry>,
    audit: Arc<dyn AuditService>,
}

impl PluginService {
    /// Construct a new service from a registry and audit sink.
    pub fn new(registry: Arc<dyn PluginRegistry>, audit: Arc<dyn AuditService>) -> Self {
        Self { registry, audit }
    }

    /// Install a verified manifest. The manifest MUST have already
    /// been signature-verified by the caller.
    pub async fn install_manifest(
        &self,
        manifest: &PluginManifest,
        actor: &str,
    ) -> Result<(), PluginInstallError> {
        manifest.capabilities.validate().map_err(|e| {
            PluginError::InvalidManifest(format!("invalid capabilities: {e}"))
        })?;
        let now = Utc::now();
        let id = manifest.id.clone();
        // If a record already exists, refuse.
        if let Some(existing) = self.registry.find(&id).await? {
            return Err(PluginInstallError::Invalid(PluginError::AlreadyInstalled(
                existing.id.to_string(),
            )));
        }
        let record = openpanel_domain::PluginRecord {
            id: manifest.id.clone(),
            version: manifest.version.clone(),
            publisher: manifest.publisher.clone(),
            status: PluginStatus::Installed,
            installed_at: now,
            enabled_at: None,
            last_error: None,
        };
        self.registry.insert(&record).await?;
        let event = AuditEvent::new(
            actor,
            AuditAction::PluginInstalled,
            AuditOutcome::Success,
        )
        .metadata(serde_json::json!({
            "id": manifest.id.as_str(),
            "version": manifest.version.as_str(),
            "publisher": manifest.publisher.as_str(),
        }));
        self.audit.record(event).await.ok();
        Ok(())
    }

    /// Enable an installed plugin.
    pub async fn enable(
        &self,
        id: &PluginId,
        actor: &str,
    ) -> Result<(), PluginInstallError> {
        let now = Utc::now();
        self.registry
            .update_status(id, PluginStatus::Enabled, None, now)
            .await?;
        let event =
            AuditEvent::new(actor, AuditAction::PluginEnabled, AuditOutcome::Success)
                .metadata(serde_json::json!({"id": id.as_str()}));
        self.audit.record(event).await.ok();
        Ok(())
    }

    /// Disable an installed plugin.
    pub async fn disable(
        &self,
        id: &PluginId,
        actor: &str,
    ) -> Result<(), PluginInstallError> {
        let now = Utc::now();
        self.registry
            .update_status(id, PluginStatus::Disabled, None, now)
            .await?;
        let event =
            AuditEvent::new(actor, AuditAction::PluginDisabled, AuditOutcome::Success)
                .metadata(serde_json::json!({"id": id.as_str()}));
        self.audit.record(event).await.ok();
        Ok(())
    }

    /// Remove a plugin from the registry.
    pub async fn uninstall(
        &self,
        id: &PluginId,
        actor: &str,
    ) -> Result<(), PluginInstallError> {
        self.registry.delete(id).await?;
        let event =
            AuditEvent::new(actor, AuditAction::PluginUninstalled, AuditOutcome::Success)
                .metadata(serde_json::json!({"id": id.as_str()}));
        self.audit.record(event).await.ok();
        Ok(())
    }

    /// List installed plugins.
    pub async fn list(&self) -> Result<Vec<openpanel_domain::PluginRecord>, PluginInstallError> {
        Ok(self.registry.list().await?)
    }

    /// Look up a plugin by id.
    pub async fn find(
        &self,
        id: &PluginId,
    ) -> Result<Option<openpanel_domain::PluginRecord>, PluginInstallError> {
        Ok(self.registry.find(id).await?)
    }
}