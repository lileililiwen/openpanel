//! Site-staging domain errors.

use thiserror::Error;

/// Domain errors returned by the `site-staging` bounded context.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum SiteStagingError {
    /// A value failed domain validation.
    #[error("invalid staging value: {0}")]
    Invalid(String),

    /// A lifecycle transition is not permitted.
    #[error("invalid staging transition")]
    InvalidTransition,

    /// The supplied document root escapes the site chroot.
    #[error("document root `{0}` is outside the site chroot")]
    OutsideChroot(String),

    /// The staging slot already exists for the site.
    #[error("staging slot already exists for site {0}")]
    AlreadyExists(String),

    /// The staging slot does not exist.
    #[error("staging slot not found for site {0}")]
    NotFound(String),

    /// A sync or promote is already in flight for the slot.
    #[error("sync or promote is already in flight")]
    InFlight,

    /// The promotion's `confirmed_at` is stale (older than the
    /// 60-second window).
    #[error("confirmation expired: {0} is older than 60 seconds")]
    ConfirmationExpired(String),

    /// A filesystem operation failed.
    #[error("filesystem error: {0}")]
    Filesystem(String),

    /// A database operation failed.
    #[error("database error: {0}")]
    Database(String),

    /// A snapshot id was supplied that does not match the slot's
    /// current snapshot.
    #[error("snapshot mismatch: expected {0}, got {1}")]
    SnapshotMismatch(String, String),
}
