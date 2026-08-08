//! `CertificateRepository` — the port through which the application
//! service persists and retrieves certificates.
//!
//! Implementation lives in `openpanel-app/src/ssl/repo.rs`
//! (`SqliteCertificateRepository`). Tests use the in-memory fake
//! from `openpanel-test-support` or hand-rolled `Arc<Mutex<HashMap>>`
//! fakes.

use async_trait::async_trait;
use uuid::Uuid;

use crate::{RepoError, ssl::certificate::Certificate};

/// Persistence port for `Certificate` aggregates.
///
/// `domain` is UNIQUE: at most one active or expired cert per domain.
/// Re-issuing for the same domain updates the existing row
/// (new `valid_from`, `valid_to`, `cert_pem`, `chain_pem`,
/// `key_pem`, `renewed_at`).
#[async_trait]
pub trait CertificateRepository: Send + Sync + 'static {
    /// Insert a new certificate row. Returns `RepoError::UniqueViolation`
    /// if a row with the same `domain` already exists.
    async fn insert(&self, cert: &Certificate) -> Result<(), RepoError>;

    /// Replace an existing row by `id`. Returns `RepoError::NotFound`
    /// if no row matches.
    async fn update(&self, cert: &Certificate) -> Result<(), RepoError>;

    /// Look up by domain. Returns `None` if no row exists for the
    /// given domain.
    async fn find_by_domain(&self, domain: &str) -> Result<Option<Certificate>, RepoError>;

    /// Look up by id. Returns `None` if no row matches.
    async fn find_by_id(&self, id: Uuid) -> Result<Option<Certificate>, RepoError>;

    /// Return every certificate, ordered by `domain` ascending.
    /// Includes `Revoked` and `Expired` rows.
    async fn list(&self) -> Result<Vec<Certificate>, RepoError>;

    /// Delete the row with the given id. Returns `Ok(())` even if no
    /// row matched.
    async fn delete(&self, id: Uuid) -> Result<(), RepoError>;
}
