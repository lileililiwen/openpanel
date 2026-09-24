//! Application-delivery service error types and projection
//! records. The service returns `ApplicationDeliveryServiceError`
//! for any lifecycle / orchestration failure; the domain types
//! from `openpanel_domain::application_delivery` are returned
//! unchanged where they apply.

use thiserror::Error;
use uuid::Uuid;

use openpanel_domain::application_delivery::ApplicationDeliveryError;

/// Service-level errors raised by `ApplicationDeliveryService`.
/// The variants intentionally mirror the domain errors plus a
/// small set of orchestration-only cases (forbidden, not found,
/// persistence). The `From<ApplicationDeliveryError>` impl keeps
/// the call sites readable.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ApplicationDeliveryServiceError {
    /// A domain validation or state-machine rule failed.
    #[error("{0}")]
    Domain(#[from] ApplicationDeliveryError),
    /// The caller is not authorised to perform the requested
    /// action.
    #[error("forbidden")]
    Forbidden,
    /// A persistence failure bubbled up from the underlying
    /// repository.
    #[error("persistence: {0}")]
    Persistence(String),
    /// A referenced application does not exist.
    #[error("application not found: {0}")]
    ApplicationNotFound(Uuid),
    /// A referenced environment does not exist.
    #[error("environment not found: {0}")]
    EnvironmentNotFound(Uuid),
    /// A referenced release does not exist.
    #[error("release not found: {0}")]
    ReleaseNotFound(Uuid),
    /// The release is not in a state that can be promoted or
    /// rolled back.
    #[error("release is not in a state that can be {0}")]
    ReleaseNotReady(&'static str),
    /// The environment is expired and can no longer accept
    /// new releases.
    #[error("environment is expired")]
    EnvironmentExpired,
}

/// In-memory projection of a registered application. The
/// projection is what the API and the CLI serialize; the
/// domain `Application` aggregate is the typed source of truth.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplicationRecord {
    /// Stable application id.
    pub id: Uuid,
    /// Owning user id.
    pub owner_id: Uuid,
    /// Display name (1..=64 chars, `[a-z0-9-]`).
    pub name: String,
    /// Stable source-kind wire name (e.g. `git`, `oci`).
    pub source_kind: String,
    /// Source URL.
    pub source_url: String,
    /// Source ref (git SHA, OCI digest, or compose SHA).
    pub source_ref: String,
    /// Build hint (may be empty for pure OCI / compose sources).
    pub build_command: String,
    /// When the application was created.
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Environments attached to the application.
    pub environments: Vec<EnvironmentRecord>,
}

/// In-memory projection of a registered environment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvironmentRecord {
    /// Stable environment id.
    pub id: Uuid,
    /// Owning application id.
    pub application_id: Uuid,
    /// Environment kind wire name (`production` or `preview`).
    pub kind: String,
    /// Logical environment name (e.g. `production`, `preview-42`).
    pub name: String,
    /// Quota ceiling (in application-defined units).
    pub quota: u32,
    /// Secret-set logical name.
    pub secret_set_name: String,
    /// Optional secret-set version.
    pub secret_set_version: Option<String>,
    /// When the environment was created.
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// When the environment expires (required for `preview`,
    /// optional for `production`).
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// In-memory projection of an immutable release.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseRecord {
    /// Stable release id.
    pub id: Uuid,
    /// Owning application id.
    pub application_id: Uuid,
    /// Target environment id.
    pub environment_id: Uuid,
    /// Immutable release digest value (e.g. `sha256:…`).
    pub digest_value: String,
    /// When the release digest was resolved.
    pub digest_resolved_at: chrono::DateTime<chrono::Utc>,
    /// Stable source-kind wire name (e.g. `git`, `oci`).
    pub source_kind: String,
    /// Source URL.
    pub source_url: String,
    /// Source ref (git SHA, OCI digest, or compose SHA).
    pub source_ref: String,
    /// Stable release-state wire name (e.g. `pending`, `healthy`).
    pub state: String,
    /// When the release was created.
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// When the release last transitioned state.
    pub last_transition_at: chrono::DateTime<chrono::Utc>,
    /// Prior healthy release id (set on rollback).
    pub previous_release_id: Option<Uuid>,
    /// Bounded, redacted diagnostic for the most recent
    /// transition.
    pub diagnostic: String,
}
