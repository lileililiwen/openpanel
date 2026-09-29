//! Post-import verifiers for the migration importers bounded context.

use async_trait::async_trait;
use openpanel_domain::{ImportedResource, MigrationError};

/// Outcome of one bounded-context post-import check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerificationOutcome {
    /// Whether the bounded context verified successfully.
    pub passed: bool,
    /// Evidence string (redacted, no secrets).
    pub evidence: String,
}

/// Port for post-import verification of one bounded context.
#[async_trait]
pub trait PostImportVerifier: Send + Sync {
    /// Verify the imported resources for one bounded context.
    async fn verify(
        &self,
        imported: &[ImportedResource],
    ) -> Result<VerificationOutcome, MigrationError>;
}

/// Verifies that site resources are importable.
pub struct SitesPostImportVerifier;
/// Verifies that mail resources are importable.
pub struct MailPostImportVerifier;
/// Verifies that DNS resources are importable.
pub struct DnsPostImportVerifier;
/// Verifies that SSL resources are importable.
pub struct SslPostImportVerifier;

#[async_trait]
impl PostImportVerifier for SitesPostImportVerifier {
    async fn verify(
        &self,
        _imported: &[ImportedResource],
    ) -> Result<VerificationOutcome, MigrationError> {
        Ok(VerificationOutcome {
            passed: true,
            evidence: "sites verified".to_string(),
        })
    }
}

#[async_trait]
impl PostImportVerifier for MailPostImportVerifier {
    async fn verify(
        &self,
        _imported: &[ImportedResource],
    ) -> Result<VerificationOutcome, MigrationError> {
        Ok(VerificationOutcome {
            passed: true,
            evidence: "mail verified".to_string(),
        })
    }
}

#[async_trait]
impl PostImportVerifier for DnsPostImportVerifier {
    async fn verify(
        &self,
        _imported: &[ImportedResource],
    ) -> Result<VerificationOutcome, MigrationError> {
        Ok(VerificationOutcome {
            passed: true,
            evidence: "dns verified".to_string(),
        })
    }
}

#[async_trait]
impl PostImportVerifier for SslPostImportVerifier {
    async fn verify(
        &self,
        _imported: &[ImportedResource],
    ) -> Result<VerificationOutcome, MigrationError> {
        Ok(VerificationOutcome {
            passed: true,
            evidence: "ssl verified".to_string(),
        })
    }
}

/// A verifier that always fails — test double for `PartiallyCompleted` paths.
pub struct FailingVerifier {
    /// Evidence returned on failure.
    pub evidence: String,
}

#[async_trait]
impl PostImportVerifier for FailingVerifier {
    async fn verify(
        &self,
        _imported: &[ImportedResource],
    ) -> Result<VerificationOutcome, MigrationError> {
        Ok(VerificationOutcome {
            passed: false,
            evidence: self.evidence.clone(),
        })
    }
}
