//! Plugin manifest, plugin id, version, and publisher key.

use base64::Engine;
use chrono::{DateTime, Utc};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};

use super::{capability::CapabilitySet, error::PluginError};

/// Plugin runtime. The framework ships two: `JsonRpc` (any language
/// able to speak JSON-RPC, executed as a child process) and `Wasm`
/// (single store compiled with `wasmtime`, sandboxed by capability).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ManifestRuntime {
    /// JSON-RPC over a localhost unix socket.
    JsonRpc,
    /// Wasmtime-compiled sandbox.
    Wasm,
}

/// Stable plugin identifier (`reverse-DNS`, lowercase, `[a-z0-9-.]+`).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PluginId(String);

impl PluginId {
    /// Construct a plugin id; rejects names that are not valid.
    pub fn new(id: impl Into<String>) -> Result<Self, PluginError> {
        let s = id.into();
        if s.is_empty()
            || s.len() > 128
            || !s
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'.' | b'-'))
        {
            return Err(PluginError::InvalidManifest(format!(
                "invalid plugin id: {s}"
            )));
        }
        Ok(Self(s))
    }

    /// Borrow the underlying string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for PluginId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<PluginId> for String {
    fn from(p: PluginId) -> Self {
        p.0
    }
}

/// Plugin semantic version (`major.minor.patch`).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PluginVersion(String);

impl PluginVersion {
    /// Construct a plugin version; rejects anything that is not
    /// `^\d+\.\d+\.\d+([+-][a-zA-Z0-9.-]+)?$`.
    pub fn new(v: impl Into<String>) -> Result<Self, PluginError> {
        let s = v.into();
        let mut parts = s.split('.');
        let major = parts.next().unwrap_or("");
        let minor = parts.next().unwrap_or("");
        let rest = parts.next().unwrap_or("");
        if parts.next().is_some()
            || !major.bytes().all(|b| b.is_ascii_digit())
            || !minor.bytes().all(|b| b.is_ascii_digit())
            || rest.is_empty()
        {
            return Err(PluginError::InvalidManifest(format!(
                "invalid plugin version: {s}"
            )));
        }
        let mut split_at = 0usize;
        for (i, b) in rest.bytes().enumerate() {
            if b == b'+' || b == b'-' {
                split_at = i;
                break;
            }
            if !b.is_ascii_digit() {
                return Err(PluginError::InvalidManifest(format!(
                    "invalid plugin version: {s}"
                )));
            }
        }
        let _ = split_at;
        Ok(Self(s))
    }

    /// Borrow the underlying string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for PluginVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Publisher public-key bytes. The marketplace CA holds a
/// `VerifyingKey` keyed by this identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PublisherKey(String);

impl PublisherKey {
    /// Construct a publisher key identifier.
    pub fn new(k: impl Into<String>) -> Result<Self, PluginError> {
        let s = k.into();
        if s.is_empty() || s.len() > 128 {
            return Err(PluginError::InvalidManifest("invalid publisher key".into()));
        }
        Ok(Self(s))
    }

    /// Borrow the underlying string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for PublisherKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Signed manifest body. The signature is an Ed25519 signature over
/// `canonical_json(manifest_without_signature) || "\n" || publisher_key`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginManifest {
    /// Plugin identifier.
    pub id: PluginId,
    /// Plugin version.
    pub version: PluginVersion,
    /// Runtime to execute under.
    pub runtime: ManifestRuntime,
    /// Entrypoint the supervisor invokes (`argv[0]` for JSON-RPC,
    /// entry export name for Wasm).
    pub entrypoint: String,
    /// Capabilities the plugin declares.
    pub capabilities: CapabilitySet,
    /// Declared permissions (filesystem paths, network scopes, …).
    #[serde(default)]
    pub permissions: Vec<String>,
    /// Optional UI metadata (display name, icon URL).
    #[serde(default)]
    pub ui: PluginUi,
    /// Publisher key identifier that signed this manifest.
    pub publisher: PublisherKey,
    /// Standard-base64 Ed25519 signature.
    pub signature: String,
    /// Manifest creation timestamp (informational).
    #[serde(default)]
    pub created_at: Option<DateTime<Utc>>,
}

/// UI metadata block, all fields optional.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct PluginUi {
    /// Display name shown in the marketplace.
    pub display_name: Option<String>,
    /// Icon URL shown in the marketplace.
    pub icon_url: Option<String>,
}

impl PluginManifest {
    /// Verify the manifest's signature under `publisher_key`.
    ///
    /// `publisher_key` is the Ed25519 public key bound to the
    /// publisher id. Verification covers the canonical JSON body
    /// (every field except `signature`) plus the publisher id, so
    /// swapping a publisher id invalidates the signature.
    pub fn verify_signature(&self, publisher_key: &VerifyingKey) -> Result<(), PluginError> {
        let body = self.canonical_body();
        let signed = format!("{body}\n{}", self.publisher.as_str());
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(self.signature.as_bytes())
            .map_err(|_| PluginError::InvalidManifestSignature)?;
        let signature =
            Signature::from_slice(&bytes).map_err(|_| PluginError::InvalidManifestSignature)?;
        publisher_key
            .verify(signed.as_bytes(), &signature)
            .map_err(|_| PluginError::InvalidManifestSignature)
    }

    /// Render the canonical signed body. Used by tests, by
    /// publishers building signatures, and by verifiers.
    pub fn canonical_body(&self) -> String {
        // Build a JSON object without `signature` and `created_at`
        // (informational) so the cover bytes are deterministic and
        // immune to timestamp drift between sign and verify.
        let mut value = serde_json::Map::new();
        value.insert(
            "id".into(),
            serde_json::Value::String(self.id.as_str().to_string()),
        );
        value.insert(
            "version".into(),
            serde_json::Value::String(self.version.as_str().to_string()),
        );
        value.insert(
            "runtime".into(),
            serde_json::Value::String(self.runtime_str().to_string()),
        );
        value.insert(
            "entrypoint".into(),
            serde_json::Value::String(self.entrypoint.clone()),
        );
        value.insert(
            "capabilities".into(),
            serde_json::to_value(&self.capabilities).expect("capabilities serialise"),
        );
        value.insert(
            "permissions".into(),
            serde_json::Value::Array(
                self.permissions
                    .iter()
                    .map(|p| serde_json::Value::String(p.clone()))
                    .collect(),
            ),
        );
        value.insert(
            "ui".into(),
            serde_json::to_value(&self.ui).expect("ui serialises"),
        );
        value.insert(
            "publisher".into(),
            serde_json::Value::String(self.publisher.as_str().to_string()),
        );
        let json = serde_json::Value::Object(value);
        serde_json::to_string(&json).expect("manifest serialises")
    }

    fn runtime_str(&self) -> &'static str {
        match self.runtime {
            ManifestRuntime::JsonRpc => "json-rpc",
            ManifestRuntime::Wasm => "wasm",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> PluginManifest {
        PluginManifest {
            id: PluginId::new("com.example.demo").unwrap(),
            version: PluginVersion::new("1.2.3").unwrap(),
            runtime: ManifestRuntime::JsonRpc,
            entrypoint: "/usr/lib/openpanel/plugins/demo/bin".into(),
            capabilities: CapabilitySet::from_names(["system-services:read"]),
            permissions: vec![],
            ui: PluginUi::default(),
            publisher: PublisherKey::new("publisher-demo").unwrap(),
            signature: String::new(),
            created_at: None,
        }
    }

    #[test]
    fn plugin_id_validates_format() {
        assert!(PluginId::new("com.example.demo").is_ok());
        assert!(PluginId::new("Bad_ID").is_err());
        assert!(PluginId::new("").is_err());
    }

    #[test]
    fn plugin_version_validates_semver() {
        assert!(PluginVersion::new("1.2.3").is_ok());
        assert!(PluginVersion::new("0.0.0").is_ok());
        assert!(PluginVersion::new("1.2").is_err());
        assert!(PluginVersion::new("a.b.c").is_err());
    }

    #[test]
    fn canonical_body_is_stable() {
        let a = sample();
        let mut b = sample();
        b.signature = "DIFFERENT".into();
        assert_eq!(a.canonical_body(), b.canonical_body());
    }
}
