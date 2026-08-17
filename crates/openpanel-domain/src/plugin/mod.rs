//! Plugin extension framework: signed manifests, capability gating,
//! lifecycle, and the supervisor-facing repository.
//!
//! This module is the *domain* layer for the plugin extension
//! framework: it owns the manifest schema, the capability tokens,
//! and the abstract `PluginRegistry` repository trait. I/O lives in
//! `openpanel-app`.
//!
//! The follow-on `refine-plugin-extension-framework-with-marketplace`
//! change layers the marketplace discovery surface on top of these
//! types.

pub mod capability;
pub mod error;
pub mod manifest;
pub mod registry;

pub use capability::{Capability, CapabilitySet};
pub use error::PluginError;
pub use manifest::{ManifestRuntime, PluginId, PluginManifest, PluginVersion, PublisherKey};
pub use registry::{PluginRecord, PluginRegistry, PluginStatus};
