//! Tar-with-JSON-manifest driver: the reference importer for the
//! panel's own deterministic backup format.
//!
//! A source bundle is a tar stream (optionally gzipped) whose
//! entries carry the resource payloads; a `manifest.json` entry at
//! the bundle root lists every resource with its kind, source key,
//! payload path, size, and dependency key. The driver sniffs the
//! bundle by finding the manifest entry, previews by parsing it,
//! and commits by walking the payload entries into the target
//! bounded contexts through the per-kind translator hook.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use openpanel_domain::{
    DriverKind, ImportedResource, ImportedResourceKind, MigrationDriver, MigrationError,
    MigrationPlan, MigrationPlanId, MigrationRunId, MigrationWarning, PlannedResource,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// One resource declaration in the manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestResource {
    /// Kind of the resource.
    pub kind: ImportedResourceKind,
    /// Natural key inside the bundle.
    pub source_key: String,
    /// Entry path inside the tar that holds the payload.
    pub payload_path: String,
    /// Declared payload size in bytes.
    pub bytes: u64,
    /// Optional dependency on another manifest resource key.
    #[serde(default)]
    pub depends_on: Option<String>,
    /// Optional descriptive text surfaced in the preview.
    #[serde(default)]
    pub description: Option<String>,
}

impl ManifestResource {
    /// Validate a declared resource. The payload path must be a
    /// plain tar entry name (no `..`, no leading slash).
    pub fn validate(&self) -> Result<(), MigrationError> {
        if self.payload_path.is_empty() || self.payload_path.len() > 1024 {
            return Err(MigrationError::MalformedSource(format!(
                "invalid payload_path for {}",
                self.source_key
            )));
        }
        if self.payload_path.starts_with('/') || self.payload_path.split('/').any(|seg| seg == "..")
        {
            return Err(MigrationError::MalformedSource(format!(
                "unsafe payload_path for {}",
                self.source_key
            )));
        }
        Ok(())
    }
}

/// The manifest document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JsonManifest {
    /// Format marker; MUST be `"openpanel-backup"`.
    pub format: String,
    /// Emitting panel version (informational).
    pub version: String,
    /// Resource declarations.
    pub resources: Vec<ManifestResource>,
}

impl JsonManifest {
    /// Validate the document shape. The format marker must match
    /// and every resource must pass its own validation.
    pub fn validate(&self) -> Result<(), MigrationError> {
        if self.format != "openpanel-backup" {
            return Err(MigrationError::MalformedSource(format!(
                "expected format openpanel-backup, got {}",
                self.format
            )));
        }
        if self.version.is_empty() {
            return Err(MigrationError::MalformedSource(
                "manifest version must not be empty".to_string(),
            ));
        }
        for resource in &self.resources {
            resource.validate()?;
        }
        Ok(())
    }
}

/// Parse the manifest document from raw bytes.
pub fn parse_manifest(bytes: &[u8]) -> Result<JsonManifest, MigrationError> {
    let manifest: JsonManifest = serde_json::from_slice(bytes)
        .map_err(|e| MigrationError::MalformedSource(format!("manifest parse error: {e}")))?;
    manifest.validate()?;
    Ok(manifest)
}

/// The manifest entry name at the bundle root.
pub const MANIFEST_ENTRY: &str = "manifest.json";

/// Sniff a raw tar (or gzip) buffer for the manifest entry.
///
/// The check is intentionally cheap: a full tar walk is deferred to
/// the driver; this function recognizes the two common forms — a
/// gzip magic prefix (`1f 8b`) or a tar header whose first entry
/// name is `manifest.json`.
pub fn sniff_tar_manifest(bytes: &[u8]) -> bool {
    if bytes.len() < 2 {
        return false;
    }
    if bytes[0] == 0x1f && bytes[1] == 0x8b {
        return true;
    }
    // A plain tar header stores the first entry name in the first
    // 100 bytes; the manifest entry may be preceded by padding
    // blocks of zeros.
    let name = MANIFEST_ENTRY.as_bytes();
    bytes.chunks(512).take(8).any(|block| {
        let head = &block[..100.min(block.len())];
        head == name
            || head
                .iter()
                .take_while(|b| **b != 0)
                .copied()
                .eq(name.iter().copied())
    })
}

/// In-memory bundle used by the reference driver: the manifest plus
/// the payload entries keyed by tar entry name.
#[derive(Debug, Clone)]
pub struct JsonManifestBundle {
    manifest: JsonManifest,
    payloads: std::collections::BTreeMap<String, Vec<u8>>,
}

impl JsonManifestBundle {
    /// Build a bundle from a manifest and payloads. Every declared
    /// payload path MUST be present.
    pub fn new(
        manifest: JsonManifest,
        payloads: std::collections::BTreeMap<String, Vec<u8>>,
    ) -> Result<Self, MigrationError> {
        manifest.validate()?;
        for resource in &manifest.resources {
            if !payloads.contains_key(&resource.payload_path) {
                return Err(MigrationError::MalformedSource(format!(
                    "missing payload {} for {}",
                    resource.payload_path, resource.source_key
                )));
            }
        }
        Ok(Self { manifest, payloads })
    }

    /// Access the manifest.
    pub fn manifest(&self) -> &JsonManifest {
        &self.manifest
    }

    /// Access a payload by tar entry name.
    pub fn payload(&self, path: &str) -> Option<&[u8]> {
        self.payloads.get(path).map(|v| v.as_slice())
    }
}

/// The reference driver for the panel's own backup format.
///
/// The commit step translates each resource by inserting a record
/// through the `Translator` hook; when no translator is registered
/// for a kind the resource is skipped and recorded in the plan.
pub struct TarWithJsonManifestDriver<T> {
    translator: T,
}

impl<T> TarWithJsonManifestDriver<T> {
    /// Build a driver over the given translator hook.
    pub fn new(translator: T) -> Self {
        Self { translator }
    }
}

/// Per-kind translator hook. The application layer supplies a
/// concrete translator that maps a payload to a target bounded
/// context and returns the new reference id.
pub trait ManifestTranslator: Send + Sync + 'static {
    /// Translate one resource payload into the target context.
    /// Returns `Ok(None)` to skip the resource (recorded in the
    /// translation log); `Ok(Some(ref_id))` to commit.
    fn translate(
        &self,
        kind: ImportedResourceKind,
        source_key: &str,
        payload: &[u8],
    ) -> Result<Option<Uuid>, String>;
}

/// A translator that refuses every kind. Used when no translator is
/// wired; every planned resource is skipped.
pub struct RefuseAll;

impl ManifestTranslator for RefuseAll {
    fn translate(
        &self,
        _kind: ImportedResourceKind,
        _source_key: &str,
        _payload: &[u8],
    ) -> Result<Option<Uuid>, String> {
        Ok(None)
    }
}

#[async_trait]
impl<T: ManifestTranslator> MigrationDriver for TarWithJsonManifestDriver<T> {
    type Source = JsonManifestBundle;

    fn sniff(&self, source: &Self::Source) -> Option<DriverKind> {
        source
            .manifest()
            .validate()
            .is_ok()
            .then_some(DriverKind::TarWithJsonManifest)
    }

    async fn dry_run(&self, source: &Self::Source) -> Result<MigrationPlan, MigrationError> {
        let manifest = source.manifest();
        let mut resources = Vec::with_capacity(manifest.resources.len());
        let conflicts = Vec::new();
        let mut warnings = Vec::new();
        for declared in &manifest.resources {
            declared.validate()?;
            let payload = source.payload(&declared.payload_path).ok_or_else(|| {
                MigrationError::MalformedSource(format!(
                    "missing payload {} for {}",
                    declared.payload_path, declared.source_key
                ))
            })?;
            let mut planned = PlannedResource::new(
                declared.kind,
                declared.source_key.clone(),
                declared
                    .description
                    .clone()
                    .unwrap_or_else(|| format!("{} import", declared.kind_name())),
                payload.len() as u64,
            )?;
            // Keep the declared byte count when it matches the
            // payload; otherwise prefer the real payload size.
            if declared.bytes == payload.len() as u64 {
                planned = PlannedResource::new(
                    declared.kind,
                    declared.source_key.clone(),
                    declared
                        .description
                        .clone()
                        .unwrap_or_else(|| format!("{} import", declared.kind_name())),
                    declared.bytes,
                )?;
            }
            #[allow(clippy::unwrap_used)] // guarded by the `is_some()` short-circuit above
            if declared.depends_on.is_some()
                && !manifest
                    .resources
                    .iter()
                    .any(|r| r.source_key == declared.depends_on.as_deref().unwrap())
            {
                warnings.push(MigrationWarning {
                    message: format!(
                        "{} depends on missing {}",
                        declared.source_key,
                        declared.depends_on.as_deref().unwrap_or("")
                    ),
                });
            }
            resources.push(planned);
        }
        Ok(MigrationPlan::new(
            MigrationPlanId::new(),
            DriverKind::TarWithJsonManifest,
            resources,
            conflicts,
            warnings,
            Utc::now(),
        ))
    }

    async fn run(
        &self,
        source: &Self::Source,
        plan: &MigrationPlan,
        run_id: MigrationRunId,
        _target_owner_user_id: Uuid,
        _confirmed_at: DateTime<Utc>,
    ) -> Result<Vec<ImportedResource>, MigrationError> {
        let manifest = source.manifest();
        let mut imported = Vec::new();
        for planned in plan.resources() {
            let payload = source
                .payload(
                    &manifest
                        .resources
                        .iter()
                        .find(|r| r.source_key == planned.source_key)
                        .ok_or_else(|| {
                            MigrationError::MalformedSource(format!(
                                "manifest lost {}",
                                planned.source_key
                            ))
                        })?
                        .payload_path,
                )
                .ok_or_else(|| {
                    MigrationError::MalformedSource(format!(
                        "missing payload for {}",
                        planned.source_key
                    ))
                })?;
            let outcome = self
                .translator
                .translate(planned.kind, &planned.source_key, payload);
            match outcome {
                Ok(Some(ref_id)) => imported.push(ImportedResource::new(
                    run_id,
                    planned.kind,
                    planned.source_key.clone(),
                    ref_id,
                )),
                Ok(None) => {
                    // Skipped: recorded in the translation log by the
                    // service; no imported resource row is written.
                }
                Err(message) => {
                    return Err(MigrationError::TargetConflict(format!(
                        "{}: {message}",
                        planned.source_key
                    )));
                }
            }
        }
        Ok(imported)
    }

    async fn rollback(
        &self,
        imported: &[ImportedResource],
        _run_id: MigrationRunId,
        _confirmed_at: DateTime<Utc>,
    ) -> Result<Vec<ImportedResource>, MigrationError> {
        // Rollback for the reference driver is a no-op at the
        // bundle level: the imported resources carry the ref ids
        // the operator undo deletes through the bounded contexts.
        // The service marks the rows rolled back.
        Ok(imported.to_vec())
    }
}

/// Extension used by the driver for a stable human label.
pub trait ManifestResourceExt {
    /// Stable human-readable label for the resource kind.
    fn kind_name(&self) -> &'static str;
}

impl ManifestResourceExt for ManifestResource {
    fn kind_name(&self) -> &'static str {
        match self.kind {
            ImportedResourceKind::User => "user",
            ImportedResourceKind::Site => "site",
            ImportedResourceKind::Database => "database",
            ImportedResourceKind::MailDomain => "mail domain",
            ImportedResourceKind::Mailbox => "mailbox",
            ImportedResourceKind::DnsZone => "DNS zone",
            ImportedResourceKind::CronJob => "cron job",
            ImportedResourceKind::SslCertificate => "SSL certificate",
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    fn valid_manifest() -> JsonManifest {
        JsonManifest {
            format: "openpanel-backup".to_string(),
            version: "1.0".to_string(),
            resources: vec![ManifestResource {
                kind: ImportedResourceKind::Site,
                source_key: "example.com".to_string(),
                payload_path: "sites/example.com".to_string(),
                bytes: 5,
                depends_on: None,
                description: Some("vhost".to_string()),
            }],
        }
    }

    #[test]
    fn validate_rejects_wrong_format() {
        let mut manifest = valid_manifest();
        manifest.format = "baota".to_string();
        let err = manifest.validate().expect_err("must reject");
        assert!(matches!(err, MigrationError::MalformedSource(_)));
    }

    #[test]
    fn validate_rejects_unsafe_payload_path() {
        let mut manifest = valid_manifest();
        manifest.resources[0].payload_path = "../etc/passwd".to_string();
        let err = manifest.validate().expect_err("must reject");
        assert!(matches!(err, MigrationError::MalformedSource(_)));
    }

    #[test]
    fn parse_manifest_round_trips() {
        let bytes = serde_json::to_vec(&valid_manifest()).unwrap();
        let parsed = parse_manifest(&bytes).unwrap();
        assert_eq!(parsed.resources.len(), 1);
        assert_eq!(parsed.resources[0].source_key, "example.com");
    }

    #[test]
    fn sniff_recognizes_gzip_prefix() {
        let mut bytes = vec![0x1f, 0x8b, 0x08];
        bytes.extend_from_slice(b"rest");
        assert!(sniff_tar_manifest(&bytes));
    }

    #[test]
    fn sniff_rejects_unrelated_bytes() {
        let bytes = b"not a tar or gzip stream at all";
        assert!(!sniff_tar_manifest(bytes));
    }

    #[test]
    fn bundle_rejects_missing_payload() {
        let manifest = valid_manifest();
        let err = JsonManifestBundle::new(manifest, BTreeMap::new()).expect_err("must reject");
        assert!(matches!(err, MigrationError::MalformedSource(_)));
    }

    #[tokio::test]
    async fn driver_preview_builds_plan() {
        let manifest = valid_manifest();
        let mut payloads = BTreeMap::new();
        payloads.insert("sites/example.com".to_string(), b"hello".to_vec());
        let bundle = JsonManifestBundle::new(manifest, payloads).unwrap();
        let driver = TarWithJsonManifestDriver::new(RefuseAll);
        assert_eq!(driver.sniff(&bundle), Some(DriverKind::TarWithJsonManifest));
        let plan = driver.dry_run(&bundle).await.unwrap();
        assert_eq!(plan.resources().len(), 1);
        assert_eq!(plan.resources()[0].source_key, "example.com");
        assert_eq!(plan.total_bytes(), 5);
    }

    #[tokio::test]
    async fn driver_run_skips_when_translator_refuses() {
        let manifest = valid_manifest();
        let mut payloads = BTreeMap::new();
        payloads.insert("sites/example.com".to_string(), b"hello".to_vec());
        let bundle = JsonManifestBundle::new(manifest, payloads).unwrap();
        let driver = TarWithJsonManifestDriver::new(RefuseAll);
        let plan = driver.dry_run(&bundle).await.unwrap();
        let imported = driver
            .run(
                &bundle,
                &plan,
                MigrationRunId::new(),
                Uuid::new_v4(),
                Utc::now(),
            )
            .await
            .unwrap();
        assert!(imported.is_empty());
    }
}
