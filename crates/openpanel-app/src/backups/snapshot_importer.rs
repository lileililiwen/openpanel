//! Snapshot importer: implements the migration-importers
//! `MigrationDriver` contract over a whole-server snapshot bundle,
//! plus offsite-target export/import glue.
//!
//! A snapshot bundle is a directory (or offsite object) holding
//! `manifest.json` and its referenced entry artifacts. The driver
//! previews the bundle as a `MigrationPlan`, commits each entry
//! through the [`SnapshotTranslator`] hook, and undoes committed
//! resources in reverse order on rollback.

use std::{collections::BTreeMap, path::Path};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use openpanel_domain::{
    DriverKind, ImportedResource, ImportedResourceKind, MigrationDriver, MigrationError,
    MigrationPlan, MigrationPlanId, MigrationRunId, PlannedResource,
    backups::snapshot::{SnapshotEntryKind, SnapshotError, SnapshotManifest},
};
use uuid::Uuid;

/// Map a snapshot entry kind onto the import-resource vocabulary.
pub fn import_kind(kind: SnapshotEntryKind) -> ImportedResourceKind {
    match kind {
        SnapshotEntryKind::PanelMetadata => ImportedResourceKind::User,
        SnapshotEntryKind::Site => ImportedResourceKind::Site,
        SnapshotEntryKind::Database => ImportedResourceKind::Database,
        SnapshotEntryKind::SslKeys => ImportedResourceKind::SslCertificate,
    }
}

/// In-memory view of one snapshot bundle: the parsed manifest plus
/// every entry artifact keyed by its manifest path.
#[derive(Debug, Clone)]
pub struct SnapshotBundleSource {
    manifest: SnapshotManifest,
    payloads: BTreeMap<String, Vec<u8>>,
}

impl SnapshotBundleSource {
    /// Build a source from a manifest and its payloads. Every
    /// declared entry path MUST be present.
    pub fn new(
        manifest: SnapshotManifest,
        payloads: BTreeMap<String, Vec<u8>>,
    ) -> Result<Self, MigrationError> {
        for entry in &manifest.entries {
            if !payloads.contains_key(&entry.path) {
                return Err(MigrationError::MalformedSource(format!(
                    "missing payload {}",
                    entry.path
                )));
            }
        }
        Ok(Self { manifest, payloads })
    }

    /// Load a bundle from a snapshot directory (`manifest.json` plus
    /// entry artifacts).
    pub fn from_dir(dir: &Path) -> Result<Self, MigrationError> {
        let manifest_bytes = std::fs::read(dir.join("manifest.json"))
            .map_err(|e| MigrationError::MalformedSource(format!("manifest read: {e}")))?;
        let manifest: SnapshotManifest = serde_json::from_slice(&manifest_bytes)
            .map_err(|e| MigrationError::MalformedSource(format!("manifest parse: {e}")))?;
        let mut payloads = BTreeMap::new();
        for entry in &manifest.entries {
            let bytes = std::fs::read(dir.join(&entry.path)).map_err(|e| {
                MigrationError::MalformedSource(format!("{} read: {e}", entry.path))
            })?;
            payloads.insert(entry.path.clone(), bytes);
        }
        Self::new(manifest, payloads)
    }

    /// Access the parsed manifest.
    pub fn manifest(&self) -> &SnapshotManifest {
        &self.manifest
    }

    /// Access an entry payload by manifest path.
    pub fn payload(&self, path: &str) -> Option<&[u8]> {
        self.payloads.get(path).map(|v| v.as_slice())
    }

    /// Serialize the whole bundle into a single gzipped tar archive
    /// suitable for offsite upload.
    pub fn to_tar_gz(&self) -> Result<Vec<u8>, MigrationError> {
        let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        let mut tar = tar::Builder::new(encoder);
        let manifest_bytes = serde_json::to_vec(&self.manifest)
            .map_err(|e| MigrationError::MalformedSource(format!("manifest serialize: {e}")))?;
        append_entry(&mut tar, "manifest.json", &manifest_bytes)?;
        for (path, bytes) in &self.payloads {
            append_entry(&mut tar, path, bytes)?;
        }
        tar.into_inner()
            .map_err(|e| MigrationError::MalformedSource(format!("tar finish: {e}")))?
            .finish()
            .map_err(|e| MigrationError::MalformedSource(format!("gzip finish: {e}")))
    }

    /// Parse a gzipped tar archive produced by [`Self::to_tar_gz`].
    pub fn from_tar_gz(bytes: &[u8]) -> Result<Self, MigrationError> {
        let decoder = flate2::read::GzDecoder::new(bytes);
        let mut tar = tar::Archive::new(decoder);
        let mut manifest: Option<SnapshotManifest> = None;
        let mut payloads = BTreeMap::new();
        for entry in tar
            .entries()
            .map_err(|e| MigrationError::MalformedSource(format!("tar walk: {e}")))?
        {
            let mut entry =
                entry.map_err(|e| MigrationError::MalformedSource(format!("tar entry: {e}")))?;
            let name = entry
                .path()
                .map_err(|e| MigrationError::MalformedSource(format!("tar path: {e}")))?
                .to_string_lossy()
                .to_string();
            let mut data = Vec::new();
            std::io::Read::read_to_end(&mut entry, &mut data)
                .map_err(|e| MigrationError::MalformedSource(format!("tar read: {e}")))?;
            if name == "manifest.json" {
                manifest = Some(serde_json::from_slice(&data).map_err(|e| {
                    MigrationError::MalformedSource(format!("manifest parse: {e}"))
                })?);
            } else {
                payloads.insert(name, data);
            }
        }
        let manifest = manifest.ok_or(MigrationError::MalformedSource(
            "bundle lacks manifest.json".into(),
        ))?;
        Self::new(manifest, payloads)
    }
}

fn append_entry(
    tar: &mut tar::Builder<flate2::write::GzEncoder<Vec<u8>>>,
    path: &str,
    bytes: &[u8],
) -> Result<(), MigrationError> {
    let mut header = tar::Header::new_gnu();
    header.set_size(bytes.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    tar.append_data(&mut header, path, bytes)
        .map_err(|e| MigrationError::MalformedSource(format!("tar append {path}: {e}")))
}

/// Per-kind commit hook. The application (or a test composition)
/// supplies a concrete translator that applies an entry payload to
/// the target bounded context and returns the new reference id.
pub trait SnapshotTranslator: Send + Sync + 'static {
    /// Commit one resource. Returns `Ok(None)` to skip the resource
    /// (recorded in the translation log); `Ok(Some(ref_id))` to
    /// commit.
    fn translate(
        &self,
        kind: ImportedResourceKind,
        source_key: &str,
        payload: &[u8],
    ) -> Result<Option<Uuid>, String>;

    /// Undo one committed resource. The default refuses; override to
    /// support rollback.
    fn rollback(
        &self,
        _kind: ImportedResourceKind,
        _source_key: &str,
        _ref_id: Uuid,
    ) -> Result<(), String> {
        Err("rollback not supported by this translator".to_string())
    }
}

/// A translator that skips every resource. Used when no real
/// translator is wired; every planned resource is recorded as
/// skipped.
pub struct SkipAll;

impl SnapshotTranslator for SkipAll {
    fn translate(
        &self,
        _kind: ImportedResourceKind,
        _source_key: &str,
        _payload: &[u8],
    ) -> Result<Option<Uuid>, String> {
        Ok(None)
    }
}

/// The whole-server snapshot importer driver.
pub struct SnapshotImporterDriver<T> {
    translator: T,
}

impl<T> SnapshotImporterDriver<T> {
    /// Build the driver over the given translator hook.
    pub fn new(translator: T) -> Self {
        Self { translator }
    }
}

#[async_trait]
impl<T: SnapshotTranslator> MigrationDriver for SnapshotImporterDriver<T> {
    type Source = SnapshotBundleSource;

    fn sniff(&self, source: &Self::Source) -> Option<DriverKind> {
        // A snapshot manifest that survives validation identifies the
        // format; entries carry sha256 digests the run re-verifies.
        (!source.manifest().entries.is_empty()).then_some(DriverKind::TarWithJsonManifest)
    }

    async fn dry_run(&self, source: &Self::Source) -> Result<MigrationPlan, MigrationError> {
        let manifest = source.manifest();
        let mut resources = Vec::with_capacity(manifest.entries.len());
        for entry in &manifest.entries {
            let payload = source.payload(&entry.path).ok_or_else(|| {
                MigrationError::MalformedSource(format!("missing payload {}", entry.path))
            })?;
            let key = entry.reference.clone().unwrap_or_else(|| match entry.kind {
                SnapshotEntryKind::PanelMetadata => "panel".to_string(),
                _ => entry.path.clone(),
            });
            resources.push(PlannedResource::new(
                import_kind(entry.kind.clone()),
                key.clone(),
                format!("snapshot entry {}", entry.path),
                payload.len() as u64,
            )?);
        }
        Ok(MigrationPlan::new(
            MigrationPlanId::new(),
            DriverKind::TarWithJsonManifest,
            resources,
            Vec::new(),
            Vec::new(),
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
        use sha2::Digest;
        let mut imported = Vec::new();
        for planned in plan.resources() {
            let entry = source
                .manifest()
                .entries
                .iter()
                .find(|e| entry_key(e) == planned.source_key)
                .ok_or_else(|| {
                    MigrationError::MalformedSource(format!("manifest lost {}", planned.source_key))
                })?;
            let payload = source.payload(&entry.path).ok_or_else(|| {
                MigrationError::MalformedSource(format!("missing payload {}", entry.path))
            })?;
            // Integrity gate: the digest must still match at commit
            // time.
            let digest = hex::encode(sha2::Sha256::digest(payload));
            if digest != entry.sha256 {
                return Err(MigrationError::MalformedSource(format!(
                    "hash mismatch for {}",
                    entry.path
                )));
            }
            if let Some(ref_id) = self
                .translator
                .translate(
                    import_kind(entry.kind.clone()),
                    planned.source_key.as_str(),
                    payload,
                )
                .map_err(MigrationError::MalformedSource)?
            {
                imported.push(ImportedResource::new(
                    run_id,
                    import_kind(entry.kind.clone()),
                    planned.source_key.clone(),
                    ref_id,
                ));
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
        let mut undone = Vec::new();
        for resource in imported.iter().rev() {
            self.translator
                .rollback(resource.kind, resource.source_key.as_str(), resource.ref_id)
                .map_err(MigrationError::MalformedSource)?;
            let mut record = resource.clone();
            record.rolled_back = true;
            undone.push(record);
        }
        Ok(undone)
    }
}

fn entry_key(entry: &openpanel_domain::backups::snapshot::SnapshotEntry) -> String {
    entry.reference.clone().unwrap_or_else(|| match entry.kind {
        SnapshotEntryKind::PanelMetadata => "panel".to_string(),
        _ => entry.path.clone(),
    })
}

/// Offsite glue: push a bundle directory to a target adapter as one
/// gzipped-tar object under `key`.
pub async fn export_to_target<A>(
    adapter: &A,
    key: &str,
    bundle_dir: &Path,
) -> Result<(), MigrationError>
where
    A: openpanel_domain::offsite_backup_targets::BackupTargetAdapter,
{
    let source = SnapshotBundleSource::from_dir(bundle_dir)?;
    let archive = source.to_tar_gz()?;
    adapter
        .put(key, &archive)
        .await
        .map_err(|e| MigrationError::MalformedSource(format!("offsite put: {e}")))
}

/// Offsite glue: pull a bundle object back into an importable
/// source.
pub async fn import_from_target<A>(
    adapter: &A,
    key: &str,
) -> Result<SnapshotBundleSource, MigrationError>
where
    A: openpanel_domain::offsite_backup_targets::BackupTargetAdapter,
{
    let bytes = adapter
        .get(key)
        .await
        .map_err(|e| MigrationError::MalformedSource(format!("offsite get: {e}")))?;
    SnapshotBundleSource::from_tar_gz(&bytes)
}

/// Re-exported so callers can surface manifest errors uniformly.
pub type SnapshotImportError = SnapshotError;
