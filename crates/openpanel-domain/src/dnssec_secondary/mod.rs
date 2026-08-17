//! DNSSEC + secondary DNS bounded context: zone signing keys,
//! secondary nameserver ACLs, glue records, and DS records.
//!
//! Private KSK / ZSK material is NEVER exposed in any DTO or
//! audit metadata — only the key tag, algorithm id, and digest
//! are surfaced.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::RepoError;

/// Errors raised by the DNSSEC + secondary DNS bounded context.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DnsSecError {
    /// The caller is not authorised.
    #[error("forbidden")]
    Forbidden,
    /// The zone is not in DNSSEC.
    #[error("zone not signed: {0}")]
    NotSigned(Uuid),
    /// The DS digest is malformed.
    #[error("invalid DS digest: {0}")]
    InvalidDigest(String),
    /// The secondary NS is rejected (bad ACL).
    #[error("secondary ns rejected: {0}")]
    SecondaryRejected(String),
    /// Persistence failed.
    #[error("persistence failed: {0}")]
    Persistence(String),
}

impl From<DnsSecError> for RepoError {
    fn from(error: DnsSecError) -> Self {
        RepoError::new(error.to_string())
    }
}

impl From<RepoError> for DnsSecError {
    fn from(error: RepoError) -> Self {
        DnsSecError::Persistence(error.0)
    }
}

/// DNSSEC signing algorithm (DNSSEC algorithm numbers from RFC 8624).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SigningAlgorithm {
    /// RSA/SHA-256 (algorithm 8).
    Rsasha256,
    /// ECDSA P-256 / SHA-256 (algorithm 13).
    Ecdsap256sha256,
    /// Ed25519 (algorithm 15).
    Ed25519,
}

impl SigningAlgorithm {
    /// Algorithm number.
    pub fn number(&self) -> u8 {
        match self {
            Self::Rsasha256 => 8,
            Self::Ecdsap256sha256 => 13,
            Self::Ed25519 => 15,
        }
    }

    /// Stable lower-case label.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Rsasha256 => "rsasha256",
            Self::Ecdsap256sha256 => "ecdsap256sha256",
            Self::Ed25519 => "ed25519",
        }
    }
}

/// Role of a signing key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyRole {
    /// Key Signing Key — signs the DNSKEY RRset.
    Ksk,
    /// Zone Signing Key — signs the rest of the zone.
    Zsk,
}

impl KeyRole {
    /// Stable lower-case label.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Ksk => "ksk",
            Self::Zsk => "zsk",
        }
    }
}

/// Per-zone DNSSEC policy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DnsSecPolicy {
    /// Owning zone id.
    pub zone_id: Uuid,
    /// Whether DNSSEC is enabled for the zone.
    pub enabled: bool,
    /// Active signing algorithm.
    pub algorithm: SigningAlgorithm,
    /// When the policy was enabled.
    pub enabled_at: Option<DateTime<Utc>>,
}

/// Public metadata for a signing key. The private key material is
/// stored separately and NEVER returned to API / CLI / UI callers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ZoneSigningKey {
    /// Stable id.
    pub id: Uuid,
    /// Owning zone.
    pub zone_id: Uuid,
    /// Role.
    pub role: KeyRole,
    /// Algorithm.
    pub algorithm: SigningAlgorithm,
    /// Public key tag (computed from the public DNSKEY).
    pub key_tag: u32,
    /// Public key digest (SHA-256, hex-encoded).
    pub public_digest: String,
    /// Whether the key is active.
    pub active: bool,
    /// Whether the key is in a rollover transition.
    pub rollover_in_progress: bool,
    /// When the key was created.
    pub created_at: DateTime<Utc>,
}

impl ZoneSigningKey {
    /// Whether the key is in a stable (non-rollover) state.
    pub fn is_stable(&self) -> bool {
        self.active && !self.rollover_in_progress
    }
}

/// Secondary nameserver entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecondaryNs {
    /// Stable id.
    pub id: Uuid,
    /// Owning zone.
    pub zone_id: Uuid,
    /// Secondary hostname or IP.
    pub address: String,
    /// AXFR ACL: allowed source IP/CIDR.
    pub allowed_cidrs: Vec<String>,
    /// When the secondary was added.
    pub added_at: DateTime<Utc>,
}

impl SecondaryNs {
    /// Whether the requester's address is in the AXFR ACL.
    pub fn allows(&self, address: &str) -> bool {
        // Trivial suffix match: the address must equal one of the
        // listed CIDRs/hosts OR end with `.<allowed>`.
        self.allowed_cidrs.iter().any(|acl| {
            acl == address
                || (acl.starts_with('.') && address.ends_with(acl))
                || (acl.contains('/') && address_in_cidr(acl, address))
        })
    }
}

fn address_in_cidr(_cidr: &str, _address: &str) -> bool {
    // For the v1 we don't ship a full CIDR matcher; the matcher
    // above is conservative and falls back to suffix / exact
    // matches. Production wiring plugs in a real matcher.
    false
}

/// Glue record for a delegation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GlueRecord {
    /// Stable id.
    pub id: Uuid,
    /// Owning zone.
    pub zone_id: Uuid,
    /// Delegated name (e.g. `ns1.example.com`).
    pub name: String,
    /// IPv4 address (A record).
    #[serde(default)]
    pub a: Option<String>,
    /// IPv6 address (AAAA record).
    #[serde(default)]
    pub aaaa: Option<String>,
}

/// DS record published to the parent zone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DsRecord {
    /// Owning zone.
    pub zone_id: Uuid,
    /// Key tag.
    pub key_tag: u32,
    /// Algorithm number.
    pub algorithm: u8,
    /// Digest type (2 = SHA-256, 4 = SHA-384).
    pub digest_type: u8,
    /// Hex-encoded digest.
    pub digest: String,
}

impl DsRecord {
    /// Validate the digest is the right length for the digest type.
    pub fn validate(&self) -> Result<(), DnsSecError> {
        let expected = match self.digest_type {
            2 => 64,
            4 => 96,
            other => {
                return Err(DnsSecError::InvalidDigest(format!(
                    "unsupported digest type {other}"
                )));
            }
        };
        if self.digest.len() != expected {
            return Err(DnsSecError::InvalidDigest(format!(
                "expected {} hex chars, got {}",
                expected,
                self.digest.len()
            )));
        }
        if !self.digest.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(DnsSecError::InvalidDigest("non-hex digest".into()));
        }
        Ok(())
    }
}

/// KSK rollover state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KskRolloverState {
    /// Single KSK is active.
    SingleActive,
    /// Old KSK is still published; new KSK is also active.
    DoubleSign,
    /// Old KSK is withdrawn from the zone; only new KSK remains.
    NewActive,
}

impl KskRolloverState {
    /// Advance the state machine.
    pub fn next(self) -> Self {
        match self {
            Self::SingleActive => Self::DoubleSign,
            Self::DoubleSign => Self::NewActive,
            Self::NewActive => Self::SingleActive,
        }
    }
}

/// Persistence port for the DNSSEC + secondary DNS bounded context.
#[async_trait]
pub trait DnsSecRepository: Send + Sync + 'static {
    /// Persist the per-zone DNSSEC policy.
    async fn save_policy(&self, policy: &DnsSecPolicy) -> Result<(), RepoError>;
    /// Load the policy for a zone.
    async fn get_policy(&self, zone_id: Uuid) -> Result<Option<DnsSecPolicy>, RepoError>;

    /// Persist a signing key.
    async fn save_key(&self, key: &ZoneSigningKey) -> Result<(), RepoError>;
    /// List keys for a zone.
    async fn list_keys(&self, zone_id: Uuid) -> Result<Vec<ZoneSigningKey>, RepoError>;

    /// Persist a secondary nameserver.
    async fn save_secondary(&self, secondary: &SecondaryNs) -> Result<(), RepoError>;
    /// List secondaries for a zone.
    async fn list_secondaries(&self, zone_id: Uuid) -> Result<Vec<SecondaryNs>, RepoError>;

    /// Persist a glue record.
    async fn save_glue(&self, glue: &GlueRecord) -> Result<(), RepoError>;
    /// List glue records for a zone.
    async fn list_glue(&self, zone_id: Uuid) -> Result<Vec<GlueRecord>, RepoError>;

    /// Persist a DS record.
    async fn save_ds(&self, ds: &DsRecord) -> Result<(), RepoError>;
    /// List DS records for a zone.
    async fn list_ds(&self, zone_id: Uuid) -> Result<Vec<DsRecord>, RepoError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn algorithm_numbers_match_rfc_8624() {
        assert_eq!(SigningAlgorithm::Rsasha256.number(), 8);
        assert_eq!(SigningAlgorithm::Ecdsap256sha256.number(), 13);
        assert_eq!(SigningAlgorithm::Ed25519.number(), 15);
    }

    #[test]
    fn ds_record_validates_digest_length() {
        let good = DsRecord {
            zone_id: Uuid::new_v4(),
            key_tag: 12345,
            algorithm: 13,
            digest_type: 2,
            digest: "a".repeat(64),
        };
        assert!(good.validate().is_ok());
        let bad = DsRecord {
            digest: "a".repeat(32),
            ..good.clone()
        };
        assert!(bad.validate().is_err());
        let bad_type = DsRecord {
            digest_type: 99,
            digest: "a".repeat(64),
            ..good.clone()
        };
        assert!(bad_type.validate().is_err());
    }

    #[test]
    fn secondary_allows_exact_match() {
        let s = SecondaryNs {
            id: Uuid::new_v4(),
            zone_id: Uuid::new_v4(),
            address: "ns1.secondary.test".into(),
            allowed_cidrs: vec!["ns1.secondary.test".into()],
            added_at: Utc::now(),
        };
        assert!(s.allows("ns1.secondary.test"));
        assert!(!s.allows("ns2.secondary.test"));
    }

    #[test]
    fn ksk_state_machine_advances() {
        use KskRolloverState::*;
        assert_eq!(SingleActive.next(), DoubleSign);
        assert_eq!(DoubleSign.next(), NewActive);
        assert_eq!(NewActive.next(), SingleActive);
    }

    #[test]
    fn signing_key_is_stable_when_active_and_no_rollover() {
        let key = ZoneSigningKey {
            id: Uuid::new_v4(),
            zone_id: Uuid::new_v4(),
            role: KeyRole::Ksk,
            algorithm: SigningAlgorithm::Ecdsap256sha256,
            key_tag: 12345,
            public_digest: "abcd".into(),
            active: true,
            rollover_in_progress: false,
            created_at: Utc::now(),
        };
        assert!(key.is_stable());
        let mut rolling = key.clone();
        rolling.rollover_in_progress = true;
        assert!(!rolling.is_stable());
    }
}
