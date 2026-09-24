//! Verified-service-catalog application layer: the typed
//! `VerifiedServiceCatalogService`, the catalog projections, and
//! the service error type. The domain types and the signature /
//! planning / lifecycle helpers live in
//! `openpanel_domain::verified_service_catalog`; this layer owns
//! the orchestration (trust registry, entry lifecycle, audit
//! fan-out), the in-memory state cache, and the SQLite migration
//! shape for persistence.
//!
//! The service NEVER returns a secret value or a manifest
//! credential material. Audit metadata is redacted by the
//! canonical `openpanel_core::audit::redact_metadata` allowlist
//! and job diagnostics are scrubbed by the deployment-adapter
//! `redact_diagnostic` contract.

mod service;
#[cfg(test)]
mod tests;
mod types;

pub use service::VerifiedServiceCatalogService;
pub use types::{CatalogEntryView, ServiceJobView, VerifiedServiceCatalogServiceError};
