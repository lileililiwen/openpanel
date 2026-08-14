//! Plugin domain errors.

use thiserror::Error;

/// Errors raised by the plugin extension framework and the
/// marketplace layer that builds on top of it.
#[derive(Debug, Error)]
pub enum PluginError {
    /// The manifest failed signature verification.
    #[error("invalid manifest signature")]
    InvalidManifestSignature,
    /// The manifest structure is invalid (missing fields, unknown
    /// capability version, etc).
    #[error("invalid manifest: {0}")]
    InvalidManifest(String),
    /// The plugin is already installed.
    #[error("plugin {0} already installed")]
    AlreadyInstalled(String),
    /// The plugin is not installed.
    #[error("plugin {0} not installed")]
    NotInstalled(String),
    /// The plugin manifest is signed by a publisher not chained to
    /// the marketplace CA.
    #[error("publisher unverified")]
    PublisherUnverified,
    /// The publisher's signature is well-formed but the catalog or
    /// manifest it covers has been tampered with.
    #[error("manifest tampered")]
    ManifestTampered,
    /// The plugin id appears in the catalog but the manifest body
    /// disagrees with the catalog entry.
    #[error("manifest url mismatch for {0}")]
    ManifestUrlMismatch(String),
    /// The plugin state machine was asked to perform an illegal
    /// transition (e.g. enabling a failed plugin).
    #[error("invalid state transition")]
    InvalidStateTransition,
    /// Generic persistence / repository failure.
    #[error("persistence error: {0}")]
    Persistence(String),
}