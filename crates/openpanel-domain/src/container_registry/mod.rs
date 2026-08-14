//! Container image registry bounded context.
//!
//! A panel-hosted OCI Distribution-compliant image registry. Per-
//! user namespaces isolate tenant image storage; pushes authenticate
//! via the panel identity; a configurable retention policy prunes
//! over-count or over-age images; an optional scan hook records
//! vulnerability findings but never auto-deletes.

pub mod config;
pub mod error;
pub mod image;
pub mod namespace;
pub mod retention;
pub mod scan;

pub use config::RegistryConfig;
pub use error::RegistryError;
pub use image::{ImageDigest, ScanStatus, StoredImage};
pub use namespace::{ImageNamespace, NamespaceId};
pub use retention::{RetentionPolicy, RetentionVerdict};
pub use scan::{ScanFinding, ScanResult};