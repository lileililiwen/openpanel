//! Marketplace catalog: signed transport envelope and entries.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::error::PluginMarketplaceError;
use super::publisher::{MarketplaceCa, PublisherSignature};

/// Catalog entry visible in the marketplace. The `manifest_url` is
/// pinned by the catalog; the panel MUST refuse to install a
/// manifest that disagrees with the URL.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MarketplacePlugin {
    /// Stable plugin id (must match `PluginManifest.id`).
    pub id: String,
    /// Display name.
    pub name: String,
    /// Publisher id, used to chain the publisher signature.
    pub publisher: String,
    /// Pinned manifest URL the catalog entry points at.
    pub manifest_url: String,
    /// Rating (`0.0..=5.0`). The panel refuses to install entries
    /// with `rating < 0.0` or `> 5.0`.
    pub rating: f32,
    /// Short summary (one paragraph).
    pub summary: String,
}

/// Plugin rating in the strict range `[0.0, 5.0]`.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PluginRating(f32);

impl PluginRating {
    /// Construct a rating; rejects NaN and out-of-range values.
    pub fn new(value: f32) -> Result<Self, PluginMarketplaceError> {
        if !value.is_finite() || !(0.0..=5.0).contains(&value) {
            return Err(PluginMarketplaceError::InvalidCatalog(format!(
                "rating out of range: {value}"
            )));
        }
        Ok(Self(value))
    }

    /// Underlying value.
    pub fn as_f32(self) -> f32 {
        self.0
    }
}

/// Signed transport envelope whose signature covers schema, expiry,
/// and exact payload bytes. Mirrors the `software_center` envelope
/// contract.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedCatalogEnvelope {
    /// Supported schema version (always 1).
    pub schema: u32,
    /// Unix timestamp after which the snapshot is rejected.
    pub expires_at: u64,
    /// Exact JSON payload (`MarketplaceCatalog`).
    pub payload: String,
    /// Standard-base64 Ed25519 signature.
    pub signature: String,
    /// Publisher id (informational; the signature itself binds to
    /// this id).
    pub publisher_id: String,
}

/// Verified marketplace catalog.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MarketplaceCatalog {
    /// SHA-256 digest of the verified payload.
    pub digest: String,
    /// Verified entries (publisher signature OK).
    pub entries: Vec<MarketplacePlugin>,
    /// Publisher id that signed the envelope.
    pub publisher_id: String,
    /// Snapshot expiry (Unix seconds).
    pub expires_at: u64,
    /// When the snapshot was last fetched (informational).
    pub fetched_at: Option<DateTime<Utc>>,
}

impl MarketplaceCatalog {
    /// Returns `true` if the catalog has expired.
    pub fn is_expired(&self, now: u64) -> bool {
        now > self.expires_at
    }

    /// Returns `true` if the catalog contains `id`.
    pub fn contains(&self, id: &str) -> bool {
        self.entries.iter().any(|p| p.id == id)
    }

    /// Look up a plugin by id.
    pub fn find(&self, id: &str) -> Option<&MarketplacePlugin> {
        self.entries.iter().find(|p| p.id == id)
    }
}

/// Schema version supported by the panel.
pub const CATALOG_SCHEMA: u32 = 1;
/// Maximum catalog payload bytes (1 MiB).
pub const CATALOG_MAX_PAYLOAD_BYTES: usize = 1_048_576;
/// Maximum catalog entries per snapshot.
pub const CATALOG_MAX_ENTRIES: usize = 1_000;

/// Verify a `SignedCatalogEnvelope` against the marketplace CA.
/// Returns the parsed catalog or a `PluginMarketplaceError`.
pub fn verify_envelope(
    envelope: &SignedCatalogEnvelope,
    ca: &MarketplaceCa,
    now: u64,
) -> Result<MarketplaceCatalog, PluginMarketplaceError> {
    if envelope.schema != CATALOG_SCHEMA {
        return Err(PluginMarketplaceError::InvalidCatalog(format!(
            "unsupported schema {}",
            envelope.schema
        )));
    }
    if now > envelope.expires_at {
        return Err(PluginMarketplaceError::InvalidCatalog(format!(
            "catalog expired at {}",
            envelope.expires_at
        )));
    }
    if envelope.payload.len() > CATALOG_MAX_PAYLOAD_BYTES {
        return Err(PluginMarketplaceError::InvalidCatalog(
            "payload too large".into(),
        ));
    }
    let signature = PublisherSignature {
        publisher_id: envelope.publisher_id.clone(),
        signature: envelope.signature.clone(),
    };
    ca.verify(&signature, envelope.payload.as_bytes())?;
    let parsed: MarketplaceCatalogPayload = serde_json::from_str(&envelope.payload)
        .map_err(|e| PluginMarketplaceError::InvalidCatalog(e.to_string()))?;
    // Strict re-serialise check: deters extra-field and reorder attacks.
    let canonical = serde_json::to_string(&parsed)
        .map_err(|e| PluginMarketplaceError::InvalidCatalog(e.to_string()))?;
    if canonical != envelope.payload {
        return Err(PluginMarketplaceError::InvalidCatalog(
            "payload not canonical".into(),
        ));
    }
    if parsed.entries.is_empty() || parsed.entries.len() > CATALOG_MAX_ENTRIES {
        return Err(PluginMarketplaceError::InvalidCatalog(
            "entries out of range".into(),
        ));
    }
    Ok(MarketplaceCatalog {
        digest: sha256_hex(envelope.payload.as_bytes()),
        entries: parsed.entries,
        publisher_id: envelope.publisher_id.clone(),
        expires_at: envelope.expires_at,
        fetched_at: None,
    })
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    hex::encode(digest)
}

/// Internal: the JSON body shape the catalog payload must take.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MarketplaceCatalogPayload {
    entries: Vec<MarketplacePlugin>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_entry() -> MarketplacePlugin {
        MarketplacePlugin {
            id: "com.example.demo".into(),
            name: "Demo plugin".into(),
            publisher: "publisher-demo".into(),
            manifest_url: "https://example.com/manifests/demo.json".into(),
            rating: 4.5,
            summary: "Adds a demo endpoint.".into(),
        }
    }

    #[test]
    fn plugin_rating_rejects_out_of_range() {
        assert!(PluginRating::new(-0.1).is_err());
        assert!(PluginRating::new(5.1).is_err());
        assert!(PluginRating::new(f32::NAN).is_err());
        assert!(PluginRating::new(0.0).is_ok());
        assert!(PluginRating::new(5.0).is_ok());
    }

    #[test]
    fn envelope_rejects_wrong_schema() {
        let ca = MarketplaceCa::empty();
        let env = SignedCatalogEnvelope {
            schema: 99,
            expires_at: u64::MAX,
            payload: "{}".into(),
            signature: String::new(),
            publisher_id: "x".into(),
        };
        assert!(verify_envelope(&env, &ca, 0).is_err());
    }
}