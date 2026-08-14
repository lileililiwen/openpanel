//! Collaborator domain errors.

use thiserror::Error;

/// Errors raised by the collaborators bounded context.
#[derive(Debug, Error)]
pub enum CollaboratorError {
    /// The collaborator could not be found.
    #[error("collaborator {0} not found")]
    NotFound(String),
    /// The site grant could not be found for the given
    /// collaborator / site pair.
    #[error("site grant missing for collaborator {0} on site {1}")]
    GrantMissing(String, String),
    /// The requested permission is outside the allowlist
    /// (file / database / mail / cron).
    #[error("permission scope not allowed: {0}")]
    ScopeNotAllowed(String),
    /// Persistence / repository failure.
    #[error("persistence error: {0}")]
    Persistence(String),
    /// The caller is not authorised to act on this site.
    #[error("not authorised for site {0}")]
    NotAuthorised(String),
}