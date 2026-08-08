//! Error types shared across the domain layer.

use thiserror::Error;

/// Persistence error abstraction. Application-layer adapters wrap their
/// database-specific error in this type so the domain layer can stay
/// database-agnostic.
#[derive(Debug, Error)]
#[error("repository error: {0}")]
pub struct RepoError(pub String);

impl RepoError {
    /// Create a new repository error from a message.
    pub fn new(msg: impl Into<String>) -> Self {
        Self(msg.into())
    }

    /// Consume the error and return the underlying message.
    pub fn into_inner(self) -> String {
        self.0
    }
}

impl From<sqlx_core_err::SqlxLike> for RepoError {
    fn from(e: sqlx_core_err::SqlxLike) -> Self {
        Self(e.0)
    }
}

/// Marker module used by the domain crate to refer to "any sqlx-like error"
/// without taking a sqlx dependency. Application adapters implement
/// `From<sqlx::Error> for RepoError` directly.
pub mod sqlx_core_err {
    /// Sqlx-like error wrapper used to avoid a direct sqlx dependency.
    #[derive(Debug)]
    pub struct SqlxLike(pub String);
    impl std::fmt::Display for SqlxLike {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str(&self.0)
        }
    }
    impl std::error::Error for SqlxLike {}
}

/// Top-level error type used across the domain layer.
#[derive(Debug, Error)]
pub enum DomainError {
    /// A validation rule was violated.
    #[error("validation error: {0}")]
    Validation(String),

    /// A persistence operation failed.
    #[error("persistence error: {0}")]
    Persistence(String),

    /// An identity error occurred.
    #[error("identity error: {0}")]
    Identity(#[from] crate::identity::error::IdentityError),

    /// The requested resource was not found.
    #[error("not found: {0}")]
    NotFound(String),

    /// A uniqueness or version conflict occurred.
    #[error("conflict: {0}")]
    Conflict(String),
}
