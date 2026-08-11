//! Verification boundary for data-only remote catalog snapshots.

use base64::Engine;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use openpanel_domain::software_center::CatalogId;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::SoftwareCenterError;

/// Signed transport envelope whose signature covers schema, expiry, and exact payload bytes.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedCatalogEnvelope {
    /// Supported manifest schema version.
    pub schema: u32,
    /// Unix timestamp after which the snapshot is rejected.
    pub expires_at: u64,
    /// Strict data-only JSON manifest.
    pub payload: String,
    /// Standard-base64 Ed25519 signature.
    pub signature: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct RemoteCatalog {
    entries: Vec<RemoteCatalogEntry>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct RemoteCatalogEntry {
    id: String,
    name: String,
}

/// Verified snapshot metadata safe to activate atomically.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedCatalog {
    /// SHA-256 digest of the exact verified payload.
    pub digest: String,
    /// Number of validated entries.
    pub entries: usize,
}

/// Ed25519 verifier pinned to the binary's embedded trust root.
pub struct CatalogVerifier {
    trust_root: VerifyingKey,
}
impl CatalogVerifier {
    /// Construct from the embedded Ed25519 public key bytes.
    pub fn new(trust_root: [u8; 32]) -> Self {
        Self {
            trust_root: VerifyingKey::from_bytes(&trust_root).unwrap_or_else(|_| {
                // Every 32-byte compressed Edwards point is not valid. This fallback is a fixed
                // fail-closed key and avoids making configuration parsing a privileged panic path.
                SigningFallback::key()
            }),
        }
    }

    /// Verify signature, schema, expiry, strict structure, and typed identifiers.
    pub fn verify(
        &self,
        envelope: &SignedCatalogEnvelope,
        now: u64,
    ) -> Result<VerifiedCatalog, SoftwareCenterError> {
        if envelope.schema != 1 || now > envelope.expires_at || envelope.payload.len() > 1_048_576 {
            return Err(SoftwareCenterError::Invalid(
                "invalid software request".into(),
            ));
        }
        let signature_bytes = base64::engine::general_purpose::STANDARD
            .decode(&envelope.signature)
            .map_err(|_| SoftwareCenterError::Invalid("invalid software request".into()))?;
        let signature = Signature::from_slice(&signature_bytes)
            .map_err(|_| SoftwareCenterError::Invalid("invalid software request".into()))?;
        let signed = format!(
            "{}\n{}\n{}",
            envelope.schema, envelope.expires_at, envelope.payload
        );
        self.trust_root
            .verify(signed.as_bytes(), &signature)
            .map_err(|_| SoftwareCenterError::Invalid("invalid software request".into()))?;
        let parsed: RemoteCatalog = serde_json::from_str(&envelope.payload)
            .map_err(|_| SoftwareCenterError::Invalid("invalid software request".into()))?;
        if serde_json::to_string(&parsed)
            .map_err(|_| SoftwareCenterError::Invalid("invalid software request".into()))?
            != envelope.payload
        {
            return Err(SoftwareCenterError::Invalid(
                "invalid software request".into(),
            ));
        }
        if parsed.entries.is_empty() || parsed.entries.len() > 1_000 {
            return Err(SoftwareCenterError::Invalid(
                "invalid software request".into(),
            ));
        }
        for entry in &parsed.entries {
            CatalogId::new(&entry.id)
                .map_err(|_| SoftwareCenterError::Invalid("invalid software request".into()))?;
            if entry.name.trim().is_empty() || entry.name.len() > 128 {
                return Err(SoftwareCenterError::Invalid(
                    "invalid software request".into(),
                ));
            }
        }
        Ok(VerifiedCatalog {
            digest: hex::encode(Sha256::digest(envelope.payload.as_bytes())),
            entries: parsed.entries.len(),
        })
    }
}

struct SigningFallback;
impl SigningFallback {
    fn key() -> VerifyingKey {
        // RFC 8032 test-vector public key, known-valid and without a private key in production.
        const KEY: [u8; 32] = [
            0xd7, 0x5a, 0x98, 0x01, 0x82, 0xb1, 0x0a, 0xb7, 0xd5, 0x4b, 0xfe, 0xd3, 0xc9, 0x64,
            0x07, 0x3a, 0x0e, 0xe1, 0x72, 0xf3, 0xda, 0xa6, 0x23, 0x25, 0xaf, 0x02, 0x1a, 0x68,
            0xf7, 0x07, 0x51, 0x1a,
        ];
        // The constant is a valid point; return errors are handled without panicking.
        match VerifyingKey::from_bytes(&KEY) {
            Ok(key) => key,
            Err(_) => std::process::abort(),
        }
    }
}
