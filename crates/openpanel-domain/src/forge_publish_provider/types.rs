//! Forge publish provider domain types: the typed wire shapes
//! for the optional `forge-publish-provider/0.1.0` provider
//! facade. The wire-level data is intentionally narrow: project
//! identity, immutable revision, operation identity, dry-run
//! flag, source-kind, secret-set reference, and bounded,
//! redacted diagnostics. Secrets and source contents are never
//! accepted or returned on the wire.
//!
//! This file is I/O-free. The provider facade in
//! `openpanel-app` translates these wire shapes into the typed
//! application-delivery and deployment-adapter operations.

use std::fmt;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

/// Stable wire name for the Forge publish provider protocol
/// version OpenPanel implements. Bumping this is a breaking
/// change; the provider MUST negotiate a compatible protocol.
pub const FORGE_PUBLISH_PROVIDER_PROTOCOL_VERSION: &str = "forge-publish-provider/0.1.0";

/// Stable wire name for the OpenPanel provider identity
/// returned in `capabilities` and echoed in every response.
pub const FORGE_PUBLISH_PROVIDER_IDENTITY: &str = "openpanel-forge-publish-provider";

/// Stable wire name for the OpenPanel provider implementation
/// version. Follows the workspace version.
pub const FORGE_PUBLISH_PROVIDER_IMPLEMENTATION_VERSION: &str = concat!(
    "openpanel-forge-publish-provider/",
    env!("CARGO_PKG_VERSION")
);

/// Forge project reference. Forge groups every publish under a
/// project; OpenPanel maps this to the owning application.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ForgeProviderProject {
    /// Stable project identifier (e.g. `acme/web`).
    pub slug: String,
    /// Optional human-readable label; never used as identity.
    pub label: Option<String>,
}

/// Immutable source-kind wire name. Mirrors
/// `application_delivery::ApplicationSourceKind` but is kept
/// independent so the Forge protocol can stay decoupled from
/// the internal source vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ForgeProviderSourceKind {
    /// Git repository pinned by SHA.
    Git,
    /// OCI image pinned by digest.
    Oci,
    /// Compose file (treated as a Git source with a compose path).
    Compose,
}

impl ForgeProviderSourceKind {
    /// Stable wire name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Git => "git",
            Self::Oci => "oci",
            Self::Compose => "compose",
        }
    }
}

impl fmt::Display for ForgeProviderSourceKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Immutable revision that the request will publish. Forge MUST
/// supply an exact ref (git SHA, OCI digest, or compose SHA);
/// tags, branches, and floating identifiers are rejected.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ForgeProviderRevision {
    /// Source kind.
    pub kind: ForgeProviderSourceKind,
    /// Source URL (e.g. `https://`, `git@`, `oci://`).
    pub url: String,
    /// Immutable source ref (git SHA, OCI digest, compose SHA).
    pub source_ref: String,
    /// Resolved release digest (e.g. `sha256:…`). Optional on
    /// `preflight`; required on `publish`.
    pub release_digest: Option<String>,
    /// Logical environment name (e.g. `production`,
    /// `preview-42`). Maps onto the application-delivery
    /// environment.
    pub environment: String,
}

impl ForgeProviderRevision {
    /// Whether the revision carries an immutable digest.
    pub fn has_immutable_digest(&self) -> bool {
        self.release_digest
            .as_ref()
            .is_some_and(|d| !d.trim().is_empty())
    }
}

/// Operation identity. Forge attaches the same operation id to
/// retries; the provider uses it for idempotency replay.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ForgeProviderOperationId {
    /// Stable Forge-side operation id (UUID).
    pub operation_id: Uuid,
    /// Optional client-supplied dedupe token.
    pub dedupe_key: Option<String>,
}

/// Secret reference. Forge never passes raw secret values; the
/// provider resolves the named set against the application's
/// environment-scoped secret store.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ForgeProviderSecretReference {
    /// Logical secret-set name (e.g. `prod`, `preview`).
    pub secret_set: String,
    /// Optional secret-set version (semver / pinned label).
    pub version: Option<String>,
}

/// Forge publish provider operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ForgeProviderOperation {
    /// Provider capabilities advertisement (no mutation).
    Capabilities,
    /// Dry-run lifecycle check.
    Preflight,
    /// Publish a new release (idempotent).
    Publish,
    /// Verify the most recent release's health.
    Verify,
    /// Rollback to the prior healthy release.
    Rollback,
}

impl ForgeProviderOperation {
    /// Stable wire name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Capabilities => "capabilities",
            Self::Preflight => "preflight",
            Self::Publish => "publish",
            Self::Verify => "verify",
            Self::Rollback => "rollback",
        }
    }

    /// Whether the operation is allowed to mutate the application.
    pub fn is_mutating(self) -> bool {
        matches!(self, Self::Publish | Self::Rollback)
    }
}

impl fmt::Display for ForgeProviderOperation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Wire-level request. The facade rejects any request that does
/// not satisfy the contract documented on the field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ForgeProviderRequest {
    /// Protocol version (e.g. `forge-publish-provider/0.1.0`).
    pub protocol: String,
    /// Operation to perform.
    pub operation: ForgeProviderOperation,
    /// Project identity.
    pub project: ForgeProviderProject,
    /// Immutable revision. Required for every operation except
    /// `capabilities`.
    pub revision: Option<ForgeProviderRevision>,
    /// Operation identity (required for every operation).
    pub operation_id: Option<ForgeProviderOperationId>,
    /// Secret-set reference (optional; resolved per environment).
    pub secret_set: Option<ForgeProviderSecretReference>,
    /// Dry-run flag (defaults to `false`).
    #[serde(default)]
    pub dry_run: bool,
}

/// Terminal status reported in every response.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ForgeProviderStatus {
    /// Operation accepted and committed (mutations) or observed
    /// (reads).
    Succeeded,
    /// Operation accepted in dry-run mode and reported the
    /// planned effect.
    DryRun,
    /// Operation rejected because the request violated the
    /// contract.
    InvalidRequest,
    /// Operation rejected because it would mutate an
    /// already-published state.
    Conflict,
    /// Operation rejected because the requested action is not
    /// supported on the target.
    Unsupported,
    /// Operation rejected because the target is currently in an
    /// unhealthy state.
    Unhealthy,
    /// Operation rejected by an internal failure. The wire
    /// response MUST NOT include the underlying cause.
    Failed,
}

impl ForgeProviderStatus {
    /// Stable wire name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Succeeded => "succeeded",
            Self::DryRun => "dry_run",
            Self::InvalidRequest => "invalid_request",
            Self::Conflict => "conflict",
            Self::Unsupported => "unsupported",
            Self::Unhealthy => "unhealthy",
            Self::Failed => "failed",
        }
    }
}

/// Health summary reported on `verify` and surfaced on every
/// non-capabilities response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForgeProviderHealthSummary {
    /// Logical environment name (e.g. `production`).
    pub environment: String,
    /// Latest release id (stable UUID; opaque to Forge).
    pub release_id: Option<Uuid>,
    /// Health wire name (`healthy`, `degraded`, `failed`,
    /// `unknown`).
    pub health: String,
    /// When the health snapshot was generated.
    pub observed_at: DateTime<Utc>,
}

/// Rollback status reported on `rollback`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForgeProviderRollbackStatus {
    /// Logical environment name.
    pub environment: String,
    /// Restored release id (UUID; opaque to Forge).
    pub restored_release_id: Option<Uuid>,
    /// Prior release id (UUID; opaque to Forge) for the
    /// release that was rolled back from.
    pub prior_release_id: Option<Uuid>,
    /// Rollback wire outcome (`succeeded`, `unsupported`,
    /// `failed`).
    pub outcome: String,
    /// When the rollback completed.
    pub completed_at: DateTime<Utc>,
}

/// Provider identity returned on `capabilities` and echoed in
/// every other response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForgeProviderProviderIdentity {
    /// Stable provider id (e.g.
    /// `openpanel-forge-publish-provider`).
    pub provider_id: String,
    /// Provider implementation version.
    pub implementation_version: String,
    /// Supported operations.
    pub operations: Vec<ForgeProviderOperation>,
    /// Protocol versions the provider speaks.
    pub protocol_versions: Vec<String>,
}

/// Terminal evidence returned on every mutating operation.
/// The diagnostic is bounded and redacted; no secret value,
/// host path, source contents, or transport command appears.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForgeProviderTerminalEvidence {
    /// Stable operation id (UUID).
    pub operation_id: Uuid,
    /// Source kind wire name.
    pub source_kind: ForgeProviderSourceKind,
    /// Logical environment name.
    pub environment: String,
    /// Resolved release digest (e.g. `sha256:…`).
    pub release_digest: String,
    /// Resulting release id (UUID; opaque to Forge).
    pub release_id: Uuid,
    /// When the terminal state was observed.
    pub completed_at: DateTime<Utc>,
    /// Bounded, redacted diagnostic (≤512 chars).
    pub diagnostic: String,
}

/// Provider capabilities returned on the `capabilities`
/// operation. The facade always returns the same shape; the
/// `operations` list and `protocol_versions` list are the
/// negotiated view.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForgeProviderCapabilities {
    /// Provider identity.
    pub provider: ForgeProviderProviderIdentity,
    /// Provider health summary (best-effort).
    pub health: ForgeProviderHealthSummary,
}

/// Wire-level response. The facade serialises exactly one of
/// the four shapes into the matching `status`:
/// `capabilities`, `health`, `rollback`, or `terminal`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForgeProviderResponse {
    /// Provider identity (echoed).
    pub provider: ForgeProviderProviderIdentity,
    /// Stable operation id (echoed or generated).
    pub operation_id: Uuid,
    /// Operation that produced this response.
    pub operation: ForgeProviderOperation,
    /// Logical environment name (when known).
    pub environment: Option<String>,
    /// Terminal status.
    pub status: ForgeProviderStatus,
    /// Capabilities payload (set when `operation == Capabilities`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<ForgeProviderCapabilities>,
    /// Health payload (set when the operation produces one).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub health: Option<ForgeProviderHealthSummary>,
    /// Rollback payload (set when `operation == Rollback`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rollback: Option<ForgeProviderRollbackStatus>,
    /// Terminal evidence (set on every mutating operation).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal: Option<ForgeProviderTerminalEvidence>,
    /// Optional bounded, redacted diagnostic.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diagnostic: Option<String>,
}

/// Typed errors raised during request parsing, validation,
/// translation, and execution. The provider facade maps every
/// error into a wire-level `ForgeProviderStatus`; the diagnostic
/// carried across the wire is always redacted.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ForgeProviderError {
    /// Protocol version is missing or unsupported.
    #[error("unsupported protocol version: {0}")]
    UnsupportedProtocol(String),
    /// Operation is missing or unsupported.
    #[error("unsupported operation: {0}")]
    UnsupportedOperation(String),
    /// Project identity is missing or malformed.
    #[error("invalid project: {0}")]
    InvalidProject(String),
    /// Revision is missing or malformed.
    #[error("invalid revision: {0}")]
    InvalidRevision(String),
    /// Operation identity is missing or malformed.
    #[error("invalid operation identity: {0}")]
    InvalidOperationIdentity(String),
    /// The application or environment does not exist.
    #[error("target not found: {0}")]
    NotFound(String),
    /// The release is not in a state that can be mutated.
    #[error("release is not ready: {0}")]
    ReleaseNotReady(String),
    /// The target is currently unhealthy.
    #[error("target is unhealthy: {0}")]
    Unhealthy(String),
    /// The requested action is not supported on the target.
    #[error("action is unsupported: {0}")]
    UnsupportedAction(String),
    /// Idempotency replay rejected the retry.
    #[error("idempotency replay rejected: {0}")]
    IdempotencyConflict(String),
    /// A typed upstream failure bubbled through (sanitised).
    #[error("provider internal failure: {0}")]
    Internal(String),
}

/// Wire-level error code carried in the response envelope.
/// Mirrors the most common `ForgeProviderStatus` variants; the
/// internal-failure path always carries `failed`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ForgeProviderErrorCode {
    /// Request violated the contract.
    InvalidRequest,
    /// The request would conflict with an existing state.
    Conflict,
    /// The requested action is not supported on the target.
    Unsupported,
    /// The target is currently unhealthy.
    Unhealthy,
    /// A non-fatal internal failure.
    NotFound,
    /// Internal failure bubbled up.
    Internal,
}

impl ForgeProviderErrorCode {
    /// Stable wire name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::InvalidRequest => "invalid_request",
            Self::Conflict => "conflict",
            Self::Unsupported => "unsupported",
            Self::Unhealthy => "unhealthy",
            Self::NotFound => "not_found",
            Self::Internal => "internal",
        }
    }
}
