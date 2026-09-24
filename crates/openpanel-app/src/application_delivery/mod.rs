//! Application delivery application layer: the typed
//! `ApplicationDeliveryService`, the SQLite repository, the
//! composition module, and the service error type. The domain
//! types and the lifecycle helpers live in
//! `openpanel_domain::application_delivery`; this layer owns the
//! orchestration (lifecycle, audit fan-out, idempotency
//! replay), the in-memory state cache, the SQLite repository,
//! and the module that wires the bounded context into the
//! registry.
//!
//! The service NEVER returns a host path, a secret value, or a
//! transport-specific command. Diagnostics are scrubbed by
//! [`openpanel_domain::application_delivery::redact_diagnostic`]
//! and audit metadata is redacted by the canonical
//! `openpanel_core::audit::redact_metadata` allowlist.

mod service;
mod types;

pub use service::ApplicationDeliveryService;
pub use types::{ApplicationDeliveryServiceError, ApplicationRecord, ReleaseRecord};
