//! Mail DKIM / SPF / DMARC defaults and mailbox quota refinement.
//!
//! This module adds the typed value objects that the mail service
//! consumes when `enable_domain` is called: a DKIM keypair (RSA
//! 2048 or Ed25519), a per-mailbox quota, and a domain sending
//! policy (outbound rate, max recipients, and SPF/DKIM/DMARC
//! required status). The actual key generator, the quota
//! enforcer, and the policy enforcer live in the application
//! layer;
//! this file owns the typed model and the validation rules.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::mail::MailError;

/// DKIM key algorithm.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DkimAlgorithm {
    /// RSA 2048.
    Rsa2048,
    /// Ed25519 (EdDSA).
    Ed25519,
}

/// A DKIM keypair. The private key is stored encrypted at rest
/// (the storage layer in the application module wraps the bytes
/// in an AES-256-GCM envelope keyed by the master key).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DkimKeypair {
    domain: String,
    selector: String,
    algorithm: DkimAlgorithm,
    private_key_blob: String,
    public_key: String,
    created_at: DateTime<Utc>,
    rotation_grace_until: Option<DateTime<Utc>>,
}

impl DkimKeypair {
    /// Build a new keypair. The selector is the DNS selector
    /// (e.g. `s1`); the private key is the encrypted blob.
    pub fn new(
        domain: impl Into<String>,
        selector: impl Into<String>,
        algorithm: DkimAlgorithm,
        private_key_blob: impl Into<String>,
        public_key: impl Into<String>,
        now: DateTime<Utc>,
    ) -> Result<Self, MailError> {
        let domain = domain.into();
        let selector = selector.into();
        if domain.is_empty() || selector.is_empty() {
            return Err(MailError::Invalid);
        }
        Ok(Self {
            domain,
            selector,
            algorithm,
            private_key_blob: private_key_blob.into(),
            public_key: public_key.into(),
            created_at: now,
            rotation_grace_until: None,
        })
    }

    /// Restore from persistence.
    pub fn restore(
        domain: String,
        selector: String,
        algorithm: DkimAlgorithm,
        private_key_blob: String,
        public_key: String,
        created_at: DateTime<Utc>,
        rotation_grace_until: Option<DateTime<Utc>>,
    ) -> Self {
        Self {
            domain,
            selector,
            algorithm,
            private_key_blob,
            public_key,
            created_at,
            rotation_grace_until,
        }
    }

    /// Domain.
    pub fn domain(&self) -> &str {
        &self.domain
    }

    /// DNS selector.
    pub fn selector(&self) -> &str {
        &self.selector
    }

    /// Algorithm.
    pub fn algorithm(&self) -> DkimAlgorithm {
        self.algorithm
    }

    /// Private key blob (encrypted).
    pub fn private_key_blob(&self) -> &str {
        &self.private_key_blob
    }

    /// Public key (PEM or base64).
    pub fn public_key(&self) -> &str {
        &self.public_key
    }

    /// When the keypair was created.
    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    /// Rotation grace until.
    pub fn rotation_grace_until(&self) -> Option<DateTime<Utc>> {
        self.rotation_grace_until
    }

    /// Begin a rotation grace window. The new keypair
    /// continues to sign for the configured window so DNS
    /// propagates before the old key is retired.
    pub fn begin_rotation(&mut self, grace_until: DateTime<Utc>) {
        self.rotation_grace_until = Some(grace_until);
    }

    /// Whether the rotation grace has elapsed at `now`.
    pub fn rotation_grace_elapsed_at(&self, now: DateTime<Utc>) -> bool {
        self.rotation_grace_until.is_some_and(|until| now >= until)
    }
}

/// Per-mailbox quota in bytes. The IMAP/MDA integration enforces
/// this on every delivery.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MailboxQuota(u64);

impl MailboxQuota {
    /// Build a quota. The minimum must be > 0 and the maximum
    /// must be ≥ minimum.
    pub fn new(value: u64, minimum: u64, maximum: u64) -> Result<Self, MailError> {
        if minimum == 0 || minimum > maximum || value < minimum || value > maximum {
            return Err(MailError::Invalid);
        }
        Ok(Self(value))
    }

    /// Quota bytes.
    pub fn bytes(self) -> u64 {
        self.0
    }

    /// Whether accepting a delivery of `incoming` bytes would
    /// exceed the quota.
    pub fn permits(self, used: u64, incoming: u64) -> bool {
        used.checked_add(incoming)
            .is_some_and(|total| total <= self.0)
    }
}

/// DKIM, SPF, and DMARC enforcement flags for outbound mail.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SendingRequirement {
    /// Disabled — the panel accepts unsigned messages.
    Disabled,
    /// SoftFail — the panel records an audit event but delivers.
    SoftFail,
    /// Required — the panel refuses unsigned messages.
    Required,
}

/// Per-domain outbound sending policy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DomainSendingPolicy {
    domain: String,
    outbound_per_minute: u32,
    max_recipients_per_message: u32,
    spf: SendingRequirement,
    dkim: SendingRequirement,
    dmarc: SendingRequirement,
}

impl DomainSendingPolicy {
    /// Default applied at `enable_domain`. Conservative numbers
    /// that match the cPanel / Baota parity.
    pub fn default_for(domain: impl Into<String>) -> Self {
        Self {
            domain: domain.into(),
            outbound_per_minute: 60,
            max_recipients_per_message: 50,
            spf: SendingRequirement::Required,
            dkim: SendingRequirement::Required,
            dmarc: SendingRequirement::SoftFail,
        }
    }

    /// Restore from persistence.
    pub fn restore(
        domain: String,
        outbound_per_minute: u32,
        max_recipients_per_message: u32,
        spf: SendingRequirement,
        dkim: SendingRequirement,
        dmarc: SendingRequirement,
    ) -> Self {
        Self {
            domain,
            outbound_per_minute,
            max_recipients_per_message,
            spf,
            dkim,
            dmarc,
        }
    }

    /// Domain.
    pub fn domain(&self) -> &str {
        &self.domain
    }

    /// Per-minute outbound limit.
    pub fn outbound_per_minute(&self) -> u32 {
        self.outbound_per_minute
    }

    /// Max recipients per message.
    pub fn max_recipients_per_message(&self) -> u32 {
        self.max_recipients_per_message
    }

    /// SPF requirement.
    pub fn spf(&self) -> SendingRequirement {
        self.spf
    }

    /// DKIM requirement.
    pub fn dkim(&self) -> SendingRequirement {
        self.dkim
    }

    /// DMARC requirement.
    pub fn dmarc(&self) -> SendingRequirement {
        self.dmarc
    }

    /// Whether an outbound message passes the policy.
    ///
    /// `passed_spf`, `passed_dkim`, and `passed_dmarc` come
    /// from the MTA's authentication results. The DMARC result
    /// depends on SPF and/or DKIM alignment per RFC 7489; the
    /// MTA forwards the alignment verdict as `passed_dmarc`.
    pub fn accepts(
        &self,
        passed_spf: bool,
        passed_dkim: bool,
        passed_dmarc: bool,
    ) -> Result<(), MailErrorSendingPolicy> {
        check_requirement(self.spf, passed_spf)?;
        check_requirement(self.dkim, passed_dkim)?;
        check_requirement(self.dmarc, passed_dmarc)?;
        Ok(())
    }
}

fn check_requirement(req: SendingRequirement, passed: bool) -> Result<(), MailErrorSendingPolicy> {
    match (req, passed) {
        (SendingRequirement::Disabled, _) => Ok(()),
        (SendingRequirement::SoftFail, _) => Ok(()),
        (SendingRequirement::Required, true) => Ok(()),
        (SendingRequirement::Required, false) => Err(MailErrorSendingPolicy::Required),
    }
}

/// Errors that can occur in the mail sending policy.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum MailErrorSendingPolicy {
    /// A required SPF/DKIM/DMARC check failed.
    #[error("a required authentication check failed")]
    Required,
}

/// Errors that can occur in the mail DKIM / quota refinement.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum MailDkimError {
    /// The DKIM keypair is malformed.
    #[error("invalid DKIM keypair: {0}")]
    InvalidKeypair(String),
    /// The rotation grace window has elapsed.
    #[error("rotation grace elapsed")]
    RotationGraceElapsed,
    /// The receiver count exceeds the per-message limit.
    #[error("recipient count exceeds policy")]
    TooManyRecipients,
    /// The outbound rate exceeds the per-minute limit.
    #[error("outbound rate exceeds policy")]
    RateExceeded,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dkim_rejects_empty_inputs() {
        let err = DkimKeypair::new("", "s1", DkimAlgorithm::Rsa2048, "blob", "pub", Utc::now())
            .expect_err("must reject");
        assert_eq!(err, MailError::Invalid);
    }

    #[test]
    fn dkim_rotation_grace_elapsed() {
        let mut keypair = DkimKeypair::new(
            "example.com",
            "s1",
            DkimAlgorithm::Ed25519,
            "blob",
            "pub",
            Utc::now(),
        )
        .unwrap();
        let past = Utc::now() - chrono::Duration::seconds(60);
        let future = Utc::now() + chrono::Duration::seconds(60);
        keypair.begin_rotation(past);
        assert!(keypair.rotation_grace_elapsed_at(Utc::now()));
        keypair.begin_rotation(future);
        assert!(!keypair.rotation_grace_elapsed_at(Utc::now()));
    }

    #[test]
    fn mailbox_quota_permits_returns_bool() {
        let quota = MailboxQuota::new(100, 10, 1000).unwrap();
        assert!(quota.permits(50, 50));
        assert!(!quota.permits(80, 30));
        assert!(quota.permits(100, 0));
    }

    #[test]
    fn sending_policy_rejects_required_dkim() {
        let policy = DomainSendingPolicy::default_for("example.com");
        let err = policy.accepts(true, false, true).expect_err("must reject");
        assert_eq!(err, MailErrorSendingPolicy::Required);
    }

    #[test]
    fn sending_policy_accepts_when_all_pass() {
        let policy = DomainSendingPolicy::default_for("example.com");
        assert!(policy.accepts(true, true, true).is_ok());
    }

    #[test]
    fn sending_policy_soft_fail_never_rejects() {
        let mut policy = DomainSendingPolicy::default_for("example.com");
        policy.spf = SendingRequirement::SoftFail;
        policy.dkim = SendingRequirement::SoftFail;
        policy.dmarc = SendingRequirement::SoftFail;
        assert!(policy.accepts(false, false, false).is_ok());
    }
}
