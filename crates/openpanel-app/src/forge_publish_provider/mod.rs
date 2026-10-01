//! Forge publish provider facade: the optional
//! `forge-publish-provider/0.1.0` provider that OpenPanel
//! exposes to Forge.
//!
//! The facade translates the wire-level Forge publish protocol
//! into existing typed application-delivery and
//! deployment-adapter operations, and never re-implements the
//! underlying state machine, audit fan-out, or rollback policy.
//!
//! Module shape:
//! - `facade` — the `ForgePublishProvider` service that owns
//!   request translation, terminal-evidence projection, and the
//!   idempotency replay cache.
//! - `protocol` — the JSON-RPC over stdio adapter and typed error
//!   mapping.
//! - `lifecycle` — the process-level start / stop harness and the
//!   `is_enabled` toggle.
//! - `transport` — the newline-delimited JSON-RPC over stdio loop
//!   that drives the facade from a Forge CLI process.
//! - `unavailable` — fail-closed `BuildAdapter` / `RuntimeAdapter`
//!   markers used until the docker-backed
//!   adapters land in the follow-up
//!   `application-delivery-docker-adapters` change.
//! - `tests` — contract tests for capabilities, preflight, publish,
//!   verify, rollback, idempotency, redaction, lifecycle, transport,
//!   and the unavailable markers.
//!
//! The provider is optional: OpenPanel starts and serves traffic
//! successfully whether the provider is enabled or not. When
//! enabled, the JSON-RPC adapter reads newline-delimited JSON
//! requests from stdin and writes the matching responses to stdout.

#![allow(clippy::module_name_repetitions)]

mod facade;
mod lifecycle;
mod protocol;
#[cfg(test)]
mod tests;
pub mod transport;
pub mod unavailable;

pub use facade::{
    ForgePublishProvider, ForgePublishProviderConfig, ForgePublishProviderError,
    ForgePublishProviderStats,
};
pub use lifecycle::{
    FORGE_PROVIDER_ENABLE_ENV, ForgeProviderProcessHandle, ForgeProviderProcessStatus,
    parse_provider_enabled,
};
pub use protocol::{
    FORGE_JSONRPC_METHOD, ForgeProviderErrorObject, ForgeProviderRequestEnvelope,
    ForgeProviderResponseEnvelope, JsonRpcCodec, JsonRpcParseError, decode_envelope,
    decode_request, encode_error, encode_response,
};

#[cfg(test)]
mod transport_tests;
