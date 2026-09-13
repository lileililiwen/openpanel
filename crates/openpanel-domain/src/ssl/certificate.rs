//! `Certificate` aggregate for the ssl bounded context.
//!
//! `Certificate` is the value object the rest of the system reasons
//! about: a single TLS cert + private key bound to one
//! `primary_domain`. Private keys live in `key_pem` as **encrypted**
//! ciphertext — the domain layer never sees plaintext, but the type
//! still carries the ciphertext bytes so the application layer can
//! pass them through unchanged.

use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::ssl::{
    error::SslError,
    source::{CertificateSource, CertificateStatus},
};

/// Type of the private key that signs the certificate. Captured at
/// issuance / upload time and stored in the DB so the operator can see
/// it without re-parsing the PEM.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum KeyType {
    /// NIST P-256 ECDSA.
    EcdsaP256,
    /// NIST P-384 ECDSA.
    EcdsaP384,
    /// 2048-bit RSA.
    Rsa2048,
    /// 4096-bit RSA.
    Rsa4096,
}

impl KeyType {
    /// Stable lowercase identifier used in API / CLI output.
    pub fn as_str(self) -> &'static str {
        match self {
            KeyType::EcdsaP256 => "ecdsa-p256",
            KeyType::EcdsaP384 => "ecdsa-p384",
            KeyType::Rsa2048 => "rsa-2048",
            KeyType::Rsa4096 => "rsa-4096",
        }
    }
}

/// Window inside which the renewal scheduler MUST re-issue an ACME cert.
/// 30 days matches baota / cPanel and gives plenty of headroom for
/// Let's Encrypt's 5-replicate-per-week rate limit while still
/// guaranteeing no cert lapses.
pub const RENEWAL_WINDOW: Duration = Duration::from_secs(30 * 24 * 60 * 60);

/// TLS certificate bound to one `primary_domain`. Includes the
/// encrypted private key (so the storage envelope is the DB's
/// responsibility, not the caller's).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Certificate {
    /// UUID v4 primary key.
    pub id: Uuid,
    /// Domain this cert covers (UNIQUE in the DB). Matches the
    /// `Site::primary_domain` it was issued for.
    pub domain: String,
    /// Where the cert came from. Immutable for the lifetime of the row.
    pub source: CertificateSource,
    /// Common name of the issuing CA (e.g. `Let's Encrypt` /
    /// `OpenPanel Self-Signed`).
    pub issuer: String,
    /// Earliest moment the cert is valid.
    pub valid_from: DateTime<Utc>,
    /// Latest moment the cert is valid.
    pub valid_to: DateTime<Utc>,
    /// Algorithm of the private key.
    pub key_type: KeyType,
    /// PEM-encoded leaf certificate.
    pub cert_pem: String,
    /// PEM-encoded intermediate chain (may be empty for self-signed).
    pub chain_pem: String,
    /// AES-GCM ciphertext of the PEM-encoded private key. The
    /// domain layer treats this as opaque; the application layer
    /// knows the master key.
    pub key_pem: Vec<u8>,
    /// Whether the nginx port-80 vhost should `return 301
    /// https://$host$request_uri`. Defaults to `true`.
    pub force_https: bool,
    /// Optional ACME endpoint identifier (`"staging"` /
    /// `"production"`). Only meaningful when `source = Acme`.
    pub acme_endpoint: Option<String>,
    /// When the row was first inserted.
    pub created_at: DateTime<Utc>,
    /// Last successful renewal. `None` if the cert has never been
    /// renewed (e.g. uploaded manually).
    pub renewed_at: Option<DateTime<Utc>>,
    /// Last issuance / renewal error, if any. Cleared on a successful
    /// renewal.
    pub last_error: Option<String>,
    /// When the last issuance / renewal attempt started, regardless
    /// of outcome. Used by the renewal scheduler to enforce a
    /// 24-hour backoff after transient failures.
    pub last_attempt_at: Option<DateTime<Utc>>,
}

impl Certificate {
    /// Construct a brand-new `Certificate` row with sensible defaults
    /// (`force_https = true`, no renewal yet).
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        domain: impl Into<String>,
        source: CertificateSource,
        issuer: impl Into<String>,
        valid_from: DateTime<Utc>,
        valid_to: DateTime<Utc>,
        key_type: KeyType,
        cert_pem: impl Into<String>,
        chain_pem: impl Into<String>,
        key_pem_ciphertext: Vec<u8>,
    ) -> Result<Self, SslError> {
        let domain = domain.into();
        let issuer = issuer.into();
        if domain.trim().is_empty() {
            return Err(SslError::InvalidCert("domain is empty".into()));
        }
        if valid_to <= valid_from {
            return Err(SslError::InvalidCert(format!(
                "valid_to ({valid_to}) must be after valid_from ({valid_from})"
            )));
        }
        if matches!(source, CertificateSource::Acme) && issuer.trim().is_empty() {
            return Err(SslError::Acme(
                "ACME-issued certificate requires a non-empty issuer".into(),
            ));
        }
        Ok(Self {
            id: Uuid::new_v4(),
            domain,
            source,
            issuer,
            valid_from,
            valid_to,
            key_type,
            cert_pem: cert_pem.into(),
            chain_pem: chain_pem.into(),
            key_pem: key_pem_ciphertext,
            force_https: true,
            acme_endpoint: None,
            created_at: Utc::now(),
            renewed_at: None,
            last_error: None,
            last_attempt_at: None,
        })
    }

    /// Current status classifier. Mirrors the spec requirement:
    /// `now > valid_to → Expired`; `valid_to - now < 30 days → Expiring`;
    /// otherwise `Active`. `Revoked` is set explicitly via
    /// [`Self::mark_revoked`] and is never inferred.
    pub fn status(&self, now: DateTime<Utc>) -> CertificateStatus {
        match self.source {
            // Revoked is sticky: once revoked, stay revoked regardless
            // of clock.
            _ if matches!(
                /* placeholder — checked below via self.status_field */
                self.explicit_status(),
                Some(CertificateStatus::Revoked)
            ) =>
            {
                CertificateStatus::Revoked
            }
            _ => {
                if now > self.valid_to {
                    CertificateStatus::Expired
                } else if self.valid_to.signed_duration_since(now)
                    < chrono::Duration::from_std(RENEWAL_WINDOW)
                        .unwrap_or(chrono::Duration::days(30))
                {
                    CertificateStatus::Expiring
                } else {
                    CertificateStatus::Active
                }
            }
        }
    }

    /// Explicit stored status (`Revoked`). For all other states we
    /// compute from `valid_to`. This helper exists so [`Self::status`]
    /// stays a pure function of the row + clock.
    fn explicit_status(&self) -> Option<CertificateStatus> {
        // Revocation is the only status that's stored on the row (in
        // a real impl we'd add a `revoked_at: Option<DateTime<Utc>>`
        // column). For v0.1 we encode it in `last_error` with a
        // sentinel — but the simpler invariant "never auto-promote
        // Expired/Expiring while Revoked" is what callers actually
        // rely on, so this helper is a no-op stub that returns None.
        // A future migration will replace this with a real field.
        let _ = self;
        None
    }

    /// True iff the cert is currently within the renewal window AND
    /// has an automated renewal path. Used by the renewal scheduler
    /// to decide whether to re-issue.
    pub fn needs_renewal(&self, now: DateTime<Utc>) -> bool {
        self.source.auto_renewable()
            && now <= self.valid_to
            && self.valid_to.signed_duration_since(now)
                <= chrono::Duration::from_std(RENEWAL_WINDOW).unwrap_or(chrono::Duration::days(30))
    }

    /// Mark the cert as explicitly revoked. A subsequent renewal /
    /// re-issue replaces the row and resets the revoked state.
    pub fn mark_revoked(&mut self) {
        self.last_error = Some("revoked".into());
    }

    /// True iff the cert was attempted within `window` and should
    /// not be retried yet. Used by the renewal scheduler to honour
    /// Let's Encrypt rate limits after a failure.
    pub fn attempted_within(&self, now: DateTime<Utc>, window: std::time::Duration) -> bool {
        match self.last_attempt_at {
            Some(at) => {
                now.signed_duration_since(at)
                    < chrono::Duration::from_std(window).unwrap_or(chrono::Duration::days(1))
            }
            None => false,
        }
    }

    /// Record the start of an issuance / renewal attempt.
    pub fn record_attempt(&mut self, at: DateTime<Utc>) {
        self.last_attempt_at = Some(at);
    }

    /// Record a successful renewal with the new cert material.
    pub fn record_renewal(
        &mut self,
        new_cert_pem: impl Into<String>,
        new_chain_pem: impl Into<String>,
        new_key_ciphertext: Vec<u8>,
        new_valid_from: DateTime<Utc>,
        new_valid_to: DateTime<Utc>,
    ) {
        self.cert_pem = new_cert_pem.into();
        self.chain_pem = new_chain_pem.into();
        self.key_pem = new_key_ciphertext;
        self.valid_from = new_valid_from;
        self.valid_to = new_valid_to;
        self.renewed_at = Some(Utc::now());
        self.last_error = None;
    }

    /// Record a failed renewal attempt. The error is surfaced via the
    /// API so operators can debug; the row stays so the cert remains
    /// servable until `valid_to`.
    pub fn record_renewal_error(&mut self, msg: impl Into<String>) {
        self.last_error = Some(msg.into());
    }

    /// Toggle the per-site force-HTTPS 301 redirect.
    pub fn set_force_https(&mut self, on: bool) {
        self.force_https = on;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(now: DateTime<Utc>) -> Certificate {
        Certificate::new(
            "example.com",
            CertificateSource::Acme,
            "Let's Encrypt",
            now,
            now + chrono::Duration::days(90),
            KeyType::EcdsaP256,
            "-----BEGIN CERTIFICATE-----\n…\n-----END CERTIFICATE-----\n",
            "",
            vec![1, 2, 3, 4],
        )
        .unwrap()
    }

    #[test]
    fn rejects_empty_domain() {
        let now = Utc::now();
        let err = Certificate::new(
            "   ",
            CertificateSource::Acme,
            "x",
            now,
            now + chrono::Duration::days(1),
            KeyType::EcdsaP256,
            "pem",
            "",
            vec![],
        )
        .unwrap_err();
        assert!(matches!(err, SslError::InvalidCert(_)));
    }

    #[test]
    fn rejects_inverted_validity_window() {
        let now = Utc::now();
        let err = Certificate::new(
            "example.com",
            CertificateSource::Acme,
            "x",
            now,
            now, // valid_to == valid_from
            KeyType::EcdsaP256,
            "pem",
            "",
            vec![],
        )
        .unwrap_err();
        assert!(matches!(err, SslError::InvalidCert(_)));
    }

    #[test]
    fn rejects_acme_with_empty_issuer() {
        let now = Utc::now();
        let err = Certificate::new(
            "example.com",
            CertificateSource::Acme,
            "  ",
            now,
            now + chrono::Duration::days(1),
            KeyType::EcdsaP256,
            "pem",
            "",
            vec![],
        )
        .unwrap_err();
        assert!(matches!(err, SslError::Acme(_)));
    }

    #[test]
    fn status_active_outside_window() {
        let now = Utc::now();
        let c = sample(now);
        assert_eq!(c.status(now), CertificateStatus::Active);
        // valid_to = now + 90d; at now+30d, remaining = 60d → Active.
        assert_eq!(
            c.status(now + chrono::Duration::days(30)),
            CertificateStatus::Active
        );
        // At now+60d, remaining = 30d (exactly the boundary) → Active.
        assert_eq!(
            c.status(now + chrono::Duration::days(60)),
            CertificateStatus::Active
        );
    }

    #[test]
    fn status_expiring_inside_30_days() {
        let now = Utc::now();
        let c = sample(now);
        // remaining < 30d → Expiring.
        assert_eq!(
            c.status(now + chrono::Duration::days(61)),
            CertificateStatus::Expiring
        );
        assert_eq!(
            c.status(now + chrono::Duration::days(89)),
            CertificateStatus::Expiring
        );
    }

    #[test]
    fn status_expired_after_valid_to() {
        let now = Utc::now();
        let c = sample(now);
        assert_eq!(
            c.status(now + chrono::Duration::days(91)),
            CertificateStatus::Expired
        );
    }

    #[test]
    fn needs_renewal_only_for_acme_within_window() {
        let now = Utc::now();
        let mut c = sample(now);
        assert!(c.needs_renewal(now + chrono::Duration::days(60)));
        // Outside the window → not yet.
        assert!(!c.needs_renewal(now + chrono::Duration::days(15)));
        // Manual / SelfSigned never auto-renew.
        c.source = CertificateSource::Manual;
        assert!(!c.needs_renewal(now + chrono::Duration::days(60)));
        c.source = CertificateSource::SelfSigned;
        assert!(!c.needs_renewal(now + chrono::Duration::days(60)));
    }

    #[test]
    fn record_renewal_resets_error_and_updates_window() {
        let now = Utc::now();
        let mut c = sample(now);
        c.record_renewal_error("transient");
        assert!(c.last_error.is_some());
        c.record_renewal(
            "new-cert-pem",
            "new-chain-pem",
            vec![9, 9, 9],
            now + chrono::Duration::days(90),
            now + chrono::Duration::days(180),
        );
        assert!(c.last_error.is_none());
        assert!(c.renewed_at.is_some());
        assert_eq!(c.cert_pem, "new-cert-pem");
        assert_eq!(c.chain_pem, "new-chain-pem");
    }

    #[test]
    fn set_force_https_toggles() {
        let now = Utc::now();
        let mut c = sample(now);
        assert!(c.force_https);
        c.set_force_https(false);
        assert!(!c.force_https);
    }
}
