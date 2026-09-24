//! Verified-service-catalog service error types and projections.
//! The service returns `VerifiedServiceCatalogServiceError` for
//! any lifecycle / orchestration failure; the domain types from
//! `openpanel_domain::verified_service_catalog` are returned
//! unchanged where they apply.

use thiserror::Error;
use uuid::Uuid;

use openpanel_domain::verified_service_catalog::{PlanConflict, VerifiedServiceCatalogError};

/// Service-level errors raised by `VerifiedServiceCatalogService`.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum VerifiedServiceCatalogServiceError {
    /// A domain validation, signature, or installability rule
    /// failed.
    #[error("{0}")]
    Domain(#[from] VerifiedServiceCatalogError),
    /// Planning refused with a specific resource conflict; no
    /// mutation was performed.
    #[error("{0}")]
    Conflict(#[from] PlanConflict),
    /// The caller is not authorised to perform the requested
    /// action.
    #[error("forbidden")]
    Forbidden,
    /// A referenced catalog entry does not exist.
    #[error("catalog entry not found: {0}")]
    EntryNotFound(String),
    /// A referenced publisher is not in the trusted registry.
    #[error("trusted publisher not found: {0}")]
    PublisherNotFound(String),
    /// A referenced lifecycle job does not exist.
    #[error("job not found: {0}")]
    JobNotFound(Uuid),
}

/// In-memory projection of one catalog entry. The projection is
/// what the review surfaces serialize; it carries trust state and
/// the installed digest, never signature key material.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogEntryView {
    /// Stable catalog entry id.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Manifest version.
    pub version: String,
    /// Declared publisher id.
    pub publisher_id: String,
    /// Trust state wire name (`verified` / `community` /
    /// `revoked` / `blocked`).
    pub trust_state: String,
    /// Whether the pinned artifact digest is currently installed.
    pub installed: bool,
    /// Digest of the installed release, when present.
    pub installed_digest: Option<String>,
}

/// In-memory projection of one observable lifecycle job.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceJobView {
    /// Job id.
    pub id: Uuid,
    /// Entry the job operates on.
    pub entry_id: String,
    /// Lifecycle phase wire name.
    pub phase: String,
    /// Outcome wire name.
    pub outcome: String,
    /// Attempts consumed.
    pub attempts: u32,
    /// Bounded retry ceiling.
    pub max_attempts: u32,
    /// Redacted last error, when present.
    pub last_error: Option<String>,
    /// Operator guidance recorded on rollback, when present.
    pub guidance: Option<String>,
}
