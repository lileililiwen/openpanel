//! Marketplace CA and publisher signature.

use ed25519_dalek::{Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};

use super::error::PluginMarketplaceError;

/// Marketplace CA: a fixed set of publisher public keys. Plugin
/// installs MUST chain to one of these.
#[derive(Debug, Clone)]
pub struct MarketplaceCa {
    keys: Vec<VerifyingKey>,
}

impl MarketplaceCa {
    /// Construct an empty CA. Empty CAs MUST refuse all installs.
    pub fn empty() -> Self {
        Self { keys: Vec::new() }
    }

    /// Construct a CA from a list of `(publisher_id, key)` pairs.
    pub fn from_keys(keys: Vec<VerifyingKey>) -> Self {
        Self { keys }
    }

    /// Returns `true` if the CA is empty.
    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    /// Returns the number of keys in the CA.
    pub fn len(&self) -> usize {
        self.keys.len()
    }

    /// Verify a publisher signature against the CA. The signature
    /// MUST chain to a key in the CA or `PublisherUnverified` is
    /// returned.
    pub fn verify(
        &self,
        publisher: &PublisherSignature,
        body: &[u8],
    ) -> Result<(), PluginMarketplaceError> {
        if self.keys.is_empty() {
            return Err(PluginMarketplaceError::CaNotConfigured);
        }
        publisher
            .verify_with_any(&self.keys, body)
            .map(|_| ())
            .ok_or(PluginMarketplaceError::PublisherUnverified)
    }
}

/// A publisher signature on a catalog entry. The signature is
/// Ed25519 over `body || "\n" || publisher_id`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublisherSignature {
    /// Publisher id (matches `MarketplaceCa` membership).
    pub publisher_id: String,
    /// Standard-base64 Ed25519 signature over the canonical body
    /// followed by the publisher id.
    pub signature: String,
}

impl PublisherSignature {
    /// Verify the signature against a single key. Useful in tests
    /// where the CA is a single key.
    pub fn verify_with(&self, key: &VerifyingKey, body: &[u8]) -> bool {
        let mut signed = body.to_vec();
        signed.push(b'\n');
        signed.extend_from_slice(self.publisher_id.as_bytes());
        let Ok(bytes) = base64::Engine::decode(
            &base64::engine::general_purpose::STANDARD,
            self.signature.as_bytes(),
        ) else {
            return false;
        };
        let Ok(sig) = ed25519_dalek::Signature::from_slice(&bytes) else {
            return false;
        };
        key.verify(&signed, &sig).is_ok()
    }

    /// Verify the signature against any of the provided keys.
    /// Returns the first key that accepted the signature, or `None`
    /// if none accepted.
    pub fn verify_with_any<'a>(
        &self,
        keys: &'a [VerifyingKey],
        body: &[u8],
    ) -> Option<&'a VerifyingKey> {
        keys.iter().find(|k| self.verify_with(k, body))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_ca_refuses_install() {
        let ca = MarketplaceCa::empty();
        let sig = PublisherSignature {
            publisher_id: "any".into(),
            signature: String::new(),
        };
        assert!(matches!(
            ca.verify(&sig, b""),
            Err(PluginMarketplaceError::CaNotConfigured)
        ));
    }
}
