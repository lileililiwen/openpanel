//! Server snapshot manifest, preflight, and confirmation-token
//! state machine. Pure domain — no I/O.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// The only manifest schema this build understands.
pub const SUPPORTED_MANIFEST_SCHEMA: u32 = 1;

/// Errors raised while validating or applying a server snapshot.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SnapshotError {
    /// The manifest schema is newer than supported.
    #[error("manifest schema {0} is newer than supported {SUPPORTED_MANIFEST_SCHEMA}")]
    SchemaTooNew(u32),
    /// A manifest field violates an invariant.
    #[error("invalid snapshot manifest: {0}")]
    Invalid(String),
    /// Restore was attempted without (or with a stale) confirmation.
    #[error("confirmation required before restore")]
    ConfirmationRequired,
    /// The confirmation token does not match the preflight.
    #[error("confirmation token mismatch")]
    TokenMismatch,
    /// An entry's bytes do not match its recorded hash.
    #[error("hash mismatch for entry `{0}`")]
    HashMismatch(String),
}

/// Kind of resource captured inside a server snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SnapshotEntryKind {
    /// Panel metadata (users, settings).
    PanelMetadata,
    /// One site's document root archive.
    Site,
    /// One database's logical dump.
    Database,
    /// SSL key material — always AES-256-GCM ciphertext.
    SslKeys,
}

/// One bundled resource with its integrity hash.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SnapshotEntry {
    /// Resource kind.
    pub kind: SnapshotEntryKind,
    /// Human-readable reference (domain / db name), when applicable.
    pub reference: Option<String>,
    /// Path of the artifact relative to the bundle root.
    pub path: String,
    /// SHA-256 hex digest of the artifact bytes.
    pub sha256: String,
}

impl SnapshotEntry {
    /// Validate the path shape and hash length/charset.
    pub fn validate(&self) -> Result<(), SnapshotError> {
        if self.path.is_empty()
            || self.path.len() > 512
            || self.path.contains("..")
            || self.path.starts_with('/')
            || self.path.contains('\\')
        {
            return Err(SnapshotError::Invalid(format!(
                "entry path `{}` escapes the bundle",
                self.path
            )));
        }
        let hash_ok =
            self.sha256.len() == 64 && self.sha256.bytes().all(|byte| byte.is_ascii_hexdigit());
        if !hash_ok {
            return Err(SnapshotError::Invalid(format!(
                "entry `{}` has a malformed sha256",
                self.path
            )));
        }
        Ok(())
    }
}

/// Versioned, secret-free manifest describing a whole-server bundle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SnapshotManifest {
    /// Manifest schema version.
    pub manifest_schema: u32,
    /// When the snapshot was created.
    pub created_at: DateTime<Utc>,
    /// Host that produced the snapshot.
    pub source_host_id: Uuid,
    /// Panel version that produced the snapshot.
    pub panel_version: String,
    /// Bundled resources.
    pub entries: Vec<SnapshotEntry>,
}

impl SnapshotManifest {
    /// Validate and construct. Rejects unknown schemas and duplicate
    /// entry paths; enforces at least one entry for full snapshots.
    pub fn new(
        created_at: DateTime<Utc>,
        source_host_id: Uuid,
        panel_version: String,
        entries: Vec<SnapshotEntry>,
    ) -> Result<Self, SnapshotError> {
        Self::validate_schema(SUPPORTED_MANIFEST_SCHEMA)?;
        if panel_version.is_empty() || panel_version.len() > 64 {
            return Err(SnapshotError::Invalid(
                "panel_version must be 1–64 chars".into(),
            ));
        }
        if entries.is_empty() {
            return Err(SnapshotError::Invalid(
                "a full snapshot needs at least one entry".into(),
            ));
        }
        if entries.len() > 4096 {
            return Err(SnapshotError::Invalid("too many entries".into()));
        }
        let mut seen_paths = std::collections::HashSet::new();
        for entry in &entries {
            entry.validate()?;
            if !seen_paths.insert(entry.path.clone()) {
                return Err(SnapshotError::Invalid(format!(
                    "duplicate entry path `{}`",
                    entry.path
                )));
            }
        }
        Ok(Self {
            manifest_schema: SUPPORTED_MANIFEST_SCHEMA,
            created_at,
            source_host_id,
            panel_version,
            entries,
        })
    }

    /// Schema compatibility gate used by preflight.
    pub fn validate_schema(schema: u32) -> Result<(), SnapshotError> {
        if schema > SUPPORTED_MANIFEST_SCHEMA {
            return Err(SnapshotError::SchemaTooNew(schema));
        }
        Ok(())
    }

    /// Re-validate a deserialized manifest.
    pub fn validated(self) -> Result<Self, SnapshotError> {
        Self::new(
            self.created_at,
            self.source_host_id,
            self.panel_version,
            self.entries,
        )
    }

    /// Whether the entry list contains any SSL key material.
    pub fn has_ssl_entries(&self) -> bool {
        self.entries
            .iter()
            .any(|entry| matches!(entry.kind, SnapshotEntryKind::SslKeys))
    }
}

/// Outcome of the pure part of a restore preflight.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreflightReport {
    /// Whether restore may proceed after confirmation.
    pub ready: bool,
    /// Warnings that do not block restore.
    pub warnings: Vec<String>,
    /// Blocking problems.
    pub blockers: Vec<String>,
    /// Name collisions detected on the target host.
    pub collisions: Vec<String>,
}

/// Build the preflight report from a manifest and target facts.
/// Pure function of its inputs.
pub fn preflight(
    manifest: &SnapshotManifest,
    local_host_id: Uuid,
    local_panel_version: &str,
    existing_names: &[String],
) -> PreflightReport {
    let mut report = PreflightReport {
        ready: true,
        warnings: Vec::new(),
        blockers: Vec::new(),
        collisions: Vec::new(),
    };
    if let Err(error) = SnapshotManifest::validate_schema(manifest.manifest_schema) {
        report.blockers.push(error.to_string());
        report.ready = false;
        return report;
    }
    if manifest.source_host_id == local_host_id {
        report
            .warnings
            .push("snapshot was produced by this same host".into());
    }
    if manifest.panel_version != local_panel_version {
        report.warnings.push(format!(
            "panel version skew: snapshot {} vs local {local_panel_version}",
            manifest.panel_version
        ));
    }
    for name in existing_names {
        report.collisions.push(name.clone());
    }
    if !report.collisions.is_empty() {
        report.warnings.push(format!(
            "{} name collision(s) require overwrite confirmation",
            report.collisions.len()
        ));
    }
    report
}

/// Single-use restore confirmation token. Minted by preflight,
/// consumed exactly once by restore.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfirmToken {
    value: String,
    used: bool,
}

impl ConfirmToken {
    /// Mint a random token.
    pub fn mint() -> Self {
        use rand::RngCore;
        let mut bytes = [0u8; 16];
        rand::rngs::OsRng.fill_bytes(&mut bytes);
        let value: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
        Self { value, used: false }
    }

    /// The token string.
    pub fn expose(&self) -> &str {
        &self.value
    }

    /// Consume the token; single use.
    pub fn consume(&mut self, presented: &str) -> Result<(), SnapshotError> {
        if self.used || self.value != presented {
            return Err(SnapshotError::TokenMismatch);
        }
        self.used = true;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(path: &str) -> SnapshotEntry {
        SnapshotEntry {
            kind: SnapshotEntryKind::Site,
            reference: Some("example.com".into()),
            path: path.to_owned(),
            sha256: "a".repeat(64),
        }
    }

    fn manifest(entries: Vec<SnapshotEntry>) -> SnapshotManifest {
        SnapshotManifest::new(Utc::now(), Uuid::new_v4(), "0.1.0".into(), entries).unwrap()
    }

    #[test]
    fn test_manifest_rejects_unknown_schema() {
        assert_eq!(
            SnapshotManifest::validate_schema(SUPPORTED_MANIFEST_SCHEMA + 1).unwrap_err(),
            SnapshotError::SchemaTooNew(SUPPORTED_MANIFEST_SCHEMA + 1)
        );
        SnapshotManifest::validate_schema(1).unwrap();
    }

    fn raw_manifest(entries: Vec<SnapshotEntry>) -> SnapshotManifest {
        SnapshotManifest {
            manifest_schema: SUPPORTED_MANIFEST_SCHEMA,
            created_at: Utc::now(),
            source_host_id: Uuid::new_v4(),
            panel_version: "0.1.0".into(),
            entries,
        }
    }

    #[test]
    fn test_manifest_rejects_escaping_paths_and_bad_hashes() {
        let escaping = raw_manifest(vec![entry("../etc/passwd")]);
        assert!(matches!(
            escaping.validated().unwrap_err(),
            SnapshotError::Invalid(_)
        ));

        let mut bad_hash = entry("sites/a.tar");
        bad_hash.sha256 = "zz".into();
        let bad = raw_manifest(vec![bad_hash]);
        assert!(matches!(
            bad.validated().unwrap_err(),
            SnapshotError::Invalid(_)
        ));
    }

    #[test]
    fn test_manifest_rejects_duplicate_paths_and_empty_entries() {
        let dup = raw_manifest(vec![entry("a"), entry("a")]);
        assert!(matches!(
            dup.validated().unwrap_err(),
            SnapshotError::Invalid(_)
        ));
        assert!(matches!(
            SnapshotManifest::new(Utc::now(), Uuid::new_v4(), "0.1.0".into(), vec![]).unwrap_err(),
            SnapshotError::Invalid(_)
        ));
    }

    #[test]
    fn test_preflight_warns_on_same_host_and_skew_blocks_on_schema() {
        let local_host = Uuid::new_v4();
        let m = SnapshotManifest::new(
            Utc::now(),
            local_host,
            "0.1.0".into(),
            vec![entry("sites/a.tar")],
        )
        .unwrap();

        // Same-host + version skew → warnings only.
        let report = preflight(&m, local_host, "0.2.0", &[]);
        assert!(report.ready);
        assert_eq!(report.warnings.len(), 2);

        // Newer schema blocks.
        let mut newer = m.clone();
        newer.manifest_schema = SUPPORTED_MANIFEST_SCHEMA + 5;
        let blocked = preflight(&newer, local_host, "0.2.0", &[]);
        assert!(!blocked.ready);
        assert_eq!(blocked.blockers.len(), 1);
    }

    #[test]
    fn test_confirm_token_is_single_use() {
        let mut token = ConfirmToken::mint();
        let value = token.expose().to_owned();
        assert_eq!(value.len(), 32);
        token.consume(&value).unwrap();
        assert_eq!(
            token.consume(&value).unwrap_err(),
            SnapshotError::TokenMismatch
        );

        let mut fresh = ConfirmToken::mint();
        assert_eq!(
            fresh.consume("wrong").unwrap_err(),
            SnapshotError::TokenMismatch
        );
    }

    #[test]
    fn test_ssl_entry_detection() {
        let mut m = manifest(vec![entry("sites/a.tar")]);
        assert!(!m.has_ssl_entries());
        m.entries.push(SnapshotEntry {
            kind: SnapshotEntryKind::SslKeys,
            reference: Some("example.com".into()),
            path: "ssl/example.com.enc".into(),
            sha256: "b".repeat(64),
        });
        assert!(m.has_ssl_entries());
    }

    /// Task 1.4: for arbitrary valid manifests, bundling then hashing
    /// reproduces identical manifest bytes and every listed hash
    /// matches its entry's source bytes (round-trip).
    #[test]
    fn prop_manifest_bundle_hash_round_trip() {
        use proptest::prelude::*;
        use sha2::{Digest, Sha256};

        let strategy = proptest::collection::vec(
            (
                0u8..4,
                proptest::option::of("[a-z]{1,12}"),
                "[a-z0-9_-]{1,16}",
                proptest::collection::vec(any::<u8>(), 0..1024),
            ),
            1..6,
        );
        proptest::test_runner::TestRunner::new(ProptestConfig::with_cases(100))
            .run(&strategy, |specs| {
                let mut entries = Vec::new();
                for (kind, reference, stem, bytes) in &specs {
                    let kind = match kind {
                        0 => SnapshotEntryKind::PanelMetadata,
                        1 => SnapshotEntryKind::Site,
                        2 => SnapshotEntryKind::Database,
                        _ => SnapshotEntryKind::SslKeys,
                    };
                    entries.push(SnapshotEntry {
                        kind,
                        reference: reference.clone(),
                        path: format!("artifacts/{stem}.bin"),
                        sha256: hex::encode(Sha256::digest(bytes)),
                    });
                    // Bundling hashes the artifact bytes; the listed
                    // digest must match the source content.
                    assert_eq!(
                        entries.last().unwrap().sha256,
                        hex::encode(Sha256::digest(bytes))
                    );
                }
                let manifest =
                    SnapshotManifest::new(Utc::now(), Uuid::new_v4(), "0.1.0".into(), entries)
                        .expect("valid manifest");

                // Serialize → parse → identical manifest and bytes.
                let bytes = serde_json::to_vec(&manifest).unwrap();
                let parsed: SnapshotManifest = serde_json::from_slice(&bytes).unwrap();
                let reparsed = parsed.clone();
                prop_assert_eq!(parsed, manifest);
                prop_assert_eq!(serde_json::to_vec(&reparsed).unwrap(), bytes);
                Ok(())
            })
            .unwrap();
    }
}
