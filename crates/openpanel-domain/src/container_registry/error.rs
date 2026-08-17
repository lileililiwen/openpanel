//! Container registry domain errors.

use thiserror::Error;

/// Errors raised by the container registry bounded context.
#[derive(Debug, Error)]
pub enum RegistryError {
    /// The actor is not authorised to act on the namespace.
    #[error("not authorised for namespace")]
    NotAuthorised,
    /// The namespace does not exist.
    #[error("namespace {0} not found")]
    NamespaceNotFound(String),
    /// The image does not exist.
    #[error("image {0} not found")]
    ImageNotFound(String),
    /// The push would exceed the namespace quota.
    #[error("namespace quota exceeded (used {used} + delta {delta} > limit {limit})")]
    QuotaExceeded {
        /// Currently used bytes.
        used: u64,
        /// Bytes the push would add.
        delta: u64,
        /// Quota limit in bytes.
        limit: u64,
    },
    /// Persistence failure.
    #[error("persistence error: {0}")]
    Persistence(String),
}
