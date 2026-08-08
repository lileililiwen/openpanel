//! Origin + lifecycle of a TLS certificate.
//!
//! These enums drive audit events, renewal decisions, and nginx render
//! behaviour (self-signed certs still serve TLS, but HSTS / force-https
//! only kick in for ACME-issued certs in practice — see the spec).

use serde::{Deserialize, Serialize};

/// Where the certificate came from. A `Certificate`'s `source` is
/// immutable: re-issuing under the same domain creates a new
/// `valid_from` / `valid_to` window on the same row, but a domain
/// cannot move from `Acme` → `Manual` without an explicit delete +
/// re-create.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CertificateSource {
    /// Issued by an ACME CA (Let's Encrypt production or staging in v0.1).
    Acme,
    /// Uploaded by the operator as PEM (cert + chain + key).
    Manual,
    /// Generated on the host by `rcgen` for dev / internal use.
    SelfSigned,
}

impl CertificateSource {
    /// True iff this source has an automated renewal path. Only `Acme`
    /// certs are auto-renewed by the renewal scheduler; manual and
    /// self-signed are the operator's responsibility.
    pub fn auto_renewable(self) -> bool {
        matches!(self, CertificateSource::Acme)
    }

    /// Stable lowercase identifier used in audit events and CLI
    /// output.
    pub fn as_str(self) -> &'static str {
        match self {
            CertificateSource::Acme => "acme",
            CertificateSource::Manual => "manual",
            CertificateSource::SelfSigned => "self_signed",
        }
    }
}

/// Lifecycle status of a certificate. The repository sets this on
/// insert and on every renewal; the renewal scheduler and the API
/// surface the value but never mutate it directly outside the
/// service.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CertificateStatus {
    /// Cert is valid and not within the renewal window.
    Active,
    /// Cert is valid but expires within 30 days; the renewal scheduler
    /// will re-issue it.
    Expiring,
    /// `now > valid_to`. nginx will refuse to load the cert on next
    /// render; the operator must re-issue.
    Expired,
    /// Explicitly revoked by the operator. The row stays so audit
    /// history is preserved; nginx no longer serves it.
    Revoked,
}

impl CertificateStatus {
    /// Stable lowercase identifier used in API / CLI output.
    pub fn as_str(self) -> &'static str {
        match self {
            CertificateStatus::Active => "active",
            CertificateStatus::Expiring => "expiring",
            CertificateStatus::Expired => "expired",
            CertificateStatus::Revoked => "revoked",
        }
    }
}
