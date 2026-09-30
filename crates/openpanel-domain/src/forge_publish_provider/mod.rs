//! Forge publish provider domain: typed wire shapes for the
//! optional `forge-publish-provider/0.1.0` provider facade.
//!
//! This module owns the *wire-level* contract that OpenPanel
//! exposes when it speaks the Forge publish protocol:
//! requests (`capabilities`, `preflight`, `publish`, `verify`,
//! `rollback`), responses (terminal status, health, rollback
//! status, redacted diagnostics, provider identity), the typed
//! error envelope, and the redaction helpers.
//!
//! The provider facade in `openpanel-app` translates these wire
//! shapes into the typed application-delivery and
//! deployment-adapter operations; it never stores secrets or
//! source contents. This file is I/O-free.
//!
//! See `logic.rs` for parsing / validation / redaction helpers
//! and `tests.rs` for the unit + property tests.

#![allow(clippy::module_name_repetitions)]

mod logic;
#[cfg(test)]
mod tests;
mod types;

pub use types::{
    FORGE_PUBLISH_PROVIDER_IDENTITY, FORGE_PUBLISH_PROVIDER_IMPLEMENTATION_VERSION,
    FORGE_PUBLISH_PROVIDER_PROTOCOL_VERSION, ForgeProviderCapabilities, ForgeProviderError,
    ForgeProviderErrorCode, ForgeProviderHealthSummary, ForgeProviderOperation,
    ForgeProviderOperationId, ForgeProviderProject, ForgeProviderProviderIdentity,
    ForgeProviderRequest, ForgeProviderResponse, ForgeProviderRevision,
    ForgeProviderRollbackStatus, ForgeProviderSecretReference, ForgeProviderSourceKind,
    ForgeProviderStatus, ForgeProviderTerminalEvidence,
};

pub use logic::{
    parse_request, redact_diagnostic, redact_request_metadata, redact_response_metadata,
    redact_source_label, validate_operation_identity, validate_request, validate_revision,
};
