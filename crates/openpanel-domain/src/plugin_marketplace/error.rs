//! Marketplace errors.

use thiserror::Error;

/// Errors raised by the marketplace layer.
#[derive(Debug, Error)]
pub enum PluginMarketplaceError {
    /// The publisher signature failed to verify against the
    /// marketplace CA.
    #[error("publisher unverified")]
    PublisherUnverified,
    /// The catalog entry was tampered after signing.
    #[error("manifest tampered")]
    ManifestTampered,
    /// The cached catalog could not be parsed.
    #[error("invalid catalog: {0}")]
    InvalidCatalog(String),
    /// The plugin id is unknown to the catalog.
    #[error("plugin {0} not in catalog")]
    UnknownPlugin(String),
    /// The plugin manifest referenced by the catalog entry did not
    /// match the catalog's pinned URL.
    #[error("manifest url mismatch for {0}")]
    ManifestUrlMismatch(String),
    /// The marketplace CA has not been configured.
    #[error("marketplace CA not configured")]
    CaNotConfigured,
    /// Persistence / I/O failure.
    #[error("persistence error: {0}")]
    Persistence(String),
    /// Underlying plugin error.
    #[error(transparent)]
    Plugin(#[from] crate::plugin::PluginError),
}