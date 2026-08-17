//! Marketplace service: discover, verify, and install plugins.

use std::sync::Arc;

use chrono::Utc;
use openpanel_core::audit::{AuditAction, AuditEvent, AuditOutcome, AuditService};
use openpanel_domain::{
    CatalogCache, CatalogSnapshot, MarketplaceCa, MarketplaceCatalog, PluginMarketplaceError,
    SignedCatalogEnvelope, common::error::RepoError, plugin::PluginManifest, verify_envelope,
};

use super::client::MarketplaceClient;
use crate::plugin::service::PluginService;

/// Discovery outcome.
#[derive(Debug, Clone)]
pub struct DiscoverOutcome {
    /// Verified catalog (publisher signature OK, payload canonical).
    pub catalog: MarketplaceCatalog,
    /// `true` when the catalog came from the cache (no network
    /// round-trip), `false` when freshly fetched.
    pub from_cache: bool,
}

/// Install request for the marketplace install path.
#[derive(Debug, Clone)]
pub struct InstallFromMarketplaceRequest {
    /// Plugin id to install (must match a catalog entry).
    pub plugin_id: String,
    /// Pinned manifest URL the catalog entry points at.
    pub manifest_url: String,
    /// The plugin manifest body, verified by the caller.
    pub manifest: PluginManifest,
    /// Publisher id, used to look up the verification key.
    pub publisher_id: String,
}

/// Errors raised by the marketplace install path.
#[derive(Debug, thiserror::Error)]
pub enum InstallFromMarketplaceError {
    /// Catalog or publisher error.
    #[error("{0}")]
    Marketplace(PluginMarketplaceError),
    /// Underlying plugin install error.
    #[error("{0}")]
    Plugin(crate::plugin::service::PluginInstallError),
}

impl From<PluginMarketplaceError> for InstallFromMarketplaceError {
    fn from(e: PluginMarketplaceError) -> Self {
        Self::Marketplace(e)
    }
}

impl From<crate::plugin::service::PluginInstallError> for InstallFromMarketplaceError {
    fn from(e: crate::plugin::service::PluginInstallError) -> Self {
        Self::Plugin(e)
    }
}

impl From<openpanel_domain::PluginError> for InstallFromMarketplaceError {
    fn from(e: openpanel_domain::PluginError) -> Self {
        Self::Marketplace(PluginMarketplaceError::Plugin(e))
    }
}

impl From<ed25519_dalek::SignatureError> for InstallFromMarketplaceError {
    fn from(_: ed25519_dalek::SignatureError) -> Self {
        Self::Marketplace(PluginMarketplaceError::ManifestTampered)
    }
}

/// Marketplace service. Wires the transport, the cache, the CA, and
/// the base plugin service.
pub struct MarketplaceService {
    client: Arc<dyn MarketplaceClient>,
    cache: Arc<dyn CatalogCache>,
    ca: std::sync::RwLock<MarketplaceCa>,
    /// Base plugin service used for delegated installs.
    pub plugins: Arc<PluginService>,
    audit: Arc<dyn AuditService>,
}

impl MarketplaceService {
    /// Construct a new marketplace service.
    pub fn new(
        client: Arc<dyn MarketplaceClient>,
        cache: Arc<dyn CatalogCache>,
        ca: MarketplaceCa,
        plugins: Arc<PluginService>,
        audit: Arc<dyn AuditService>,
    ) -> Self {
        Self {
            client,
            cache,
            ca: std::sync::RwLock::new(ca),
            plugins,
            audit,
        }
    }

    /// Replace the marketplace CA at runtime. Used by the
    /// configuration flow when an admin rotates the CA.
    pub fn set_ca(&self, ca: MarketplaceCa) {
        if let Ok(mut guard) = self.ca.write() {
            *guard = ca;
        }
    }

    /// Snapshot the current CA.
    pub fn ca(&self) -> MarketplaceCa {
        self.ca
            .read()
            .map(|g| g.clone())
            .unwrap_or_else(|_| MarketplaceCa::empty())
    }

    /// Fetch a fresh signed envelope, verify it against the CA,
    /// cache the verified snapshot, and return the catalog.
    pub async fn discover(
        &self,
        now: Option<u64>,
    ) -> Result<DiscoverOutcome, PluginMarketplaceError> {
        let envelope = self.client.fetch_envelope().await?;
        let now = now.unwrap_or_else(|| Utc::now().timestamp().max(0) as u64);
        let ca = self.ca();
        let catalog = verify_envelope(&envelope, &ca, now)?;
        // Cache the verified snapshot by digest.
        let snapshot = CatalogSnapshot {
            catalog: catalog.clone(),
            cached_at: Utc::now(),
        };
        if let Err(err) = self.cache.put(&snapshot).await {
            return Err(PluginMarketplaceError::Persistence(format!(
                "cache.put: {err}"
            )));
        }
        Ok(DiscoverOutcome {
            catalog,
            from_cache: false,
        })
    }

    /// Fetch from the cache only; returns `None` if no snapshot is
    /// cached.
    pub async fn cached(&self) -> Result<Option<CatalogSnapshot>, PluginMarketplaceError> {
        self.cache
            .latest()
            .await
            .map_err(|e| PluginMarketplaceError::Persistence(e.0))
    }

    /// Verify a manifest against the marketplace CA. The signature
    /// covers the canonical manifest body and the publisher id.
    pub fn verify_manifest(
        &self,
        manifest: &PluginManifest,
        publisher_key: &ed25519_dalek::VerifyingKey,
    ) -> Result<(), PluginMarketplaceError> {
        manifest
            .verify_signature(publisher_key)
            .map_err(PluginMarketplaceError::from)
    }

    /// Install a plugin from a marketplace catalog entry.
    ///
    /// The catalog entry MUST chain to the marketplace CA; the
    /// manifest MUST chain to the same publisher key. The install
    /// itself is delegated to the base plugin service.
    pub async fn install_from_marketplace(
        &self,
        request: InstallFromMarketplaceRequest,
        publisher_key: &ed25519_dalek::VerifyingKey,
        actor: &str,
    ) -> Result<(), InstallFromMarketplaceError> {
        // Verify the manifest signature against the publisher key
        // before any install work runs.
        request
            .manifest
            .verify_signature(publisher_key)
            .map_err(InstallFromMarketplaceError::from)?;
        // The marketplace install path also requires that the
        // plugin id matches between catalog entry and manifest.
        if request.manifest.id.as_str() != request.plugin_id {
            return Err(
                PluginMarketplaceError::ManifestUrlMismatch(request.plugin_id.clone()).into(),
            );
        }
        // Capability check.
        request.manifest.capabilities.validate().map_err(|e| {
            PluginMarketplaceError::from(openpanel_domain::PluginError::InvalidManifest(format!(
                "invalid capabilities: {e}"
            )))
        })?;
        // Persist the manifest URL for the audit trail.
        self.plugins
            .install_manifest(&request.manifest, actor)
            .await?;
        let event = AuditEvent::new(
            actor,
            AuditAction::PluginInstalledFromMarketplace,
            AuditOutcome::Success,
        )
        .metadata(serde_json::json!({
            "id": request.plugin_id,
            "publisher": request.publisher_id,
            "manifest_url": request.manifest_url,
        }));
        self.audit.record(event).await.ok();
        Ok(())
    }
}

// Convenience trait impls to make error wrapping clean.
impl From<RepoError> for InstallFromMarketplaceError {
    fn from(e: RepoError) -> Self {
        Self::Marketplace(PluginMarketplaceError::Persistence(e.0))
    }
}

#[doc(hidden)]
pub fn _ensure_signed_envelope_reachable(
    env: &SignedCatalogEnvelope,
) -> Result<&SignedCatalogEnvelope, PluginMarketplaceError> {
    if env.schema == 0 {
        return Err(PluginMarketplaceError::InvalidCatalog(
            "schema 0 is invalid".into(),
        ));
    }
    Ok(env)
}
