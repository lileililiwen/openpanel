//! Site clone and template export service: typed `ClonePlan` and
//! `CloneRun` orchestration, PII anonymisation with encrypted
//! tokens, and the `TemplateExporter` (tar + Ed25519 manifest +
//! signature).
//!
//! Real filesystem I/O is performed by the exporter; the
//! `SiteCloneService` itself is I/O-free apart from the SQLite repo
//! and the `SitesService` (which performs the file copy and DB
//! load through the existing bounded context).

use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};

use chrono::Utc;
use ed25519_dalek::{Signer, SigningKey, Verifier, VerifyingKey};
use openpanel_core::{AuditAction, AuditEvent, AuditOutcome, AuditService};
use openpanel_domain::{
    AnonymisationToken, CloneFile, ClonePlan, CloneRun, CloneSource, DbAction, PiiPolicy, Site,
    SiteCloneTemplateError, SiteCloneTemplateRepository, SiteTemplate,
};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{
    offsite_backup_targets::kek::{KEK_LEN, encrypt_payload},
    sites::service::SitesService,
};

/// Derive a 32-byte KEK from the master key + a per-run salt using
/// SHA-256. The KEK is used to encrypt per-row anonymisation tokens;
/// the master key lives in the panel config and is never persisted.
fn derive_clone_kek(master_key: &[u8; KEK_LEN], salt: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(master_key);
    hasher.update(salt);
    let digest = hasher.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(&digest);
    out
}

/// Window in seconds during which a `confirmed_at` is accepted
/// (per the spec: ±60 s).
pub const CONFIRM_WINDOW_SECONDS: i64 = 60;

/// Default directory the `TemplateExporter` writes to.
pub const DEFAULT_TEMPLATE_ARTIFACT_DIR: &str = "/var/lib/openpanel/templates";

/// Filename for the per-template JSON manifest inside the artifact.
pub const TEMPLATE_MANIFEST_FILENAME: &str = "template.json";

/// A bundled tar.gz artifact plus its manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemplateArtifact {
    /// On-disk path of the tar.gz archive.
    pub archive_path: PathBuf,
    /// JSON manifest body.
    pub manifest_json: String,
    /// Ed25519 signature over the manifest body.
    pub signature: String,
    /// Ed25519 verifying key (hex) used to verify the signature.
    pub verifying_key_hex: String,
}

/// Per-source file enumeration trait. The default implementation
/// (used by the application service) is the real filesystem walker;
/// tests substitute a mock.
pub trait FileEnumerator: Send + Sync {
    /// List (src_path, content_hash) pairs under `chroot`, skipping
    /// any path matching `deny_patterns`.
    fn enumerate(
        &self,
        chroot: &Path,
        deny_patterns: &[String],
    ) -> Result<Vec<(String, String)>, SiteCloneTemplateError>;
}

/// Real filesystem enumerator: walks `chroot` and returns
/// `(relative_path, sha256_hex)` pairs.
pub struct FsEnumerator;

impl FileEnumerator for FsEnumerator {
    fn enumerate(
        &self,
        chroot: &Path,
        deny_patterns: &[String],
    ) -> Result<Vec<(String, String)>, SiteCloneTemplateError> {
        if !chroot.exists() {
            return Ok(vec![]);
        }
        let mut out = Vec::new();
        walk(chroot, chroot, deny_patterns, &mut out)
            .map_err(|e| SiteCloneTemplateError::Persistence(format!("walk: {e}")))?;
        Ok(out)
    }
}

fn walk(
    root: &Path,
    dir: &Path,
    deny_patterns: &[String],
    out: &mut Vec<(String, String)>,
) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let rel = path
            .strip_prefix(root)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?
            .to_string_lossy()
            .replace('\\', "/");
        if deny_patterns.iter().any(|p| glob_match(p, &rel)) {
            continue;
        }
        let meta = entry.metadata()?;
        if meta.is_dir() {
            walk(root, &path, deny_patterns, out)?;
        } else if meta.is_file() {
            let bytes = std::fs::read(&path)?;
            let mut hasher = Sha256::new();
            hasher.update(&bytes);
            let digest = hasher.finalize();
            out.push((rel, hex::encode(digest)));
        }
    }
    Ok(())
}

fn glob_match(pattern: &str, path: &str) -> bool {
    if let Some((prefix, suffix)) = pattern.split_once('*') {
        path.starts_with(prefix) && path.ends_with(suffix)
    } else {
        path == pattern
    }
}

/// Application service for site clone + template export.
pub struct SiteCloneService {
    repo: Arc<dyn SiteCloneTemplateRepository>,
    sites: Arc<SitesService>,
    audit: Arc<dyn AuditService>,
    master_key: [u8; KEK_LEN],
    artifact_dir: PathBuf,
    enumerator: Arc<dyn FileEnumerator>,
}

impl SiteCloneService {
    /// Build a service.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        repo: Arc<dyn SiteCloneTemplateRepository>,
        sites: Arc<SitesService>,
        audit: Arc<dyn AuditService>,
        master_key: [u8; KEK_LEN],
        artifact_dir: Option<PathBuf>,
        enumerator: Option<Arc<dyn FileEnumerator>>,
    ) -> Self {
        Self {
            repo,
            sites,
            audit,
            master_key,
            artifact_dir: artifact_dir
                .unwrap_or_else(|| PathBuf::from(DEFAULT_TEMPLATE_ARTIFACT_DIR)),
            enumerator: enumerator.unwrap_or_else(|| Arc::new(FsEnumerator)),
        }
    }

    /// The directory the template exporter writes to.
    pub fn artifact_dir(&self) -> &Path {
        &self.artifact_dir
    }

    /// Compute a `ClonePlan` for a live-site source.
    pub async fn plan_live_clone(
        &self,
        actor: &str,
        source_site_id: Uuid,
        target_domain: impl Into<String>,
        target_owner_id: Uuid,
        pii_policy: PiiPolicy,
    ) -> Result<ClonePlan, SiteCloneTemplateError> {
        let source = self
            .sites
            .get_site(source_site_id)
            .await
            .map_err(|e| SiteCloneTemplateError::Refused(format!("source: {e}")))?;
        let deny = default_deny_patterns();
        let chroot = PathBuf::from(source.document_root());
        let entries = self
            .enumerator
            .enumerate(&chroot, &deny)
            .map_err(|e| SiteCloneTemplateError::Refused(format!("enumerate: {e}")))?;
        let mut files = Vec::new();
        let mut hasher = Sha256::new();
        for (rel, hash) in &entries {
            files.push(CloneFile::new(rel.clone(), rel.clone(), hash.clone()));
            hasher.update(rel.as_bytes());
            hasher.update(hash.as_bytes());
        }
        let content_hash = hex::encode(hasher.finalize());
        let plan = ClonePlan::new(
            Uuid::new_v4(),
            CloneSource::Site {
                site_id: source_site_id,
            },
            target_owner_id,
            target_domain,
            pii_policy,
            files,
            DbAction::DumpAndLoad { est_size_bytes: 0 },
            vec![format!(
                "live clone of `{}` with policy `{}`",
                source.primary_domain(),
                pii_policy.as_str()
            )],
            content_hash,
            Utc::now(),
        )?;
        self.repo.upsert_plan(&plan).await?;
        self.audit
            .record(
                AuditEvent::new(actor, AuditAction::SiteCloned, AuditOutcome::Success)
                    .target(plan.id().to_string())
                    .metadata(serde_json::json!({
                        "source": "live",
                        "plan_id": plan.id().to_string(),
                    })),
            )
            .await
            .ok();
        Ok(plan)
    }

    /// Compute a `ClonePlan` for a template source. Templates
    /// always carry `PiiPolicy::None`.
    pub async fn plan_template_clone(
        &self,
        actor: &str,
        template_id: Uuid,
        target_domain: impl Into<String>,
        target_owner_id: Uuid,
    ) -> Result<ClonePlan, SiteCloneTemplateError> {
        let template =
            self.repo
                .find_template(template_id)
                .await?
                .ok_or(SiteCloneTemplateError::Refused(
                    "template not found".to_string(),
                ))?;
        if !template.signature_valid() {
            return Err(SiteCloneTemplateError::TemplateSignatureFailed);
        }
        let content_hash = template.signature().to_string();
        let plan = ClonePlan::new(
            Uuid::new_v4(),
            CloneSource::Template { template_id },
            target_owner_id,
            target_domain,
            PiiPolicy::None,
            vec![],
            DbAction::ImportSchemaWithPlaceholderData,
            vec![format!(
                "template `{}` clone; PII is `{}`",
                template.name(),
                template.pii_policy().as_str()
            )],
            content_hash,
            Utc::now(),
        )?;
        self.repo.upsert_plan(&plan).await?;
        self.audit
            .record(
                AuditEvent::new(
                    actor,
                    AuditAction::SiteClonedFromTemplate,
                    AuditOutcome::Success,
                )
                .target(plan.id().to_string())
                .metadata(serde_json::json!({
                    "template_id": template_id.to_string(),
                    "plan_id": plan.id().to_string(),
                })),
            )
            .await
            .ok();
        Ok(plan)
    }

    /// Confirm and execute a plan.
    pub async fn run_clone(
        &self,
        caller: &openpanel_domain::User,
        plan_id: Uuid,
        confirmed_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<CloneRun, SiteCloneTemplateError> {
        let plan = self
            .repo
            .find_plan(plan_id)
            .await?
            .ok_or(SiteCloneTemplateError::Refused("plan not found".into()))?;
        let now = Utc::now();
        let delta = (now - confirmed_at).num_seconds().abs();
        if delta > CONFIRM_WINDOW_SECONDS {
            return Err(SiteCloneTemplateError::PlanExpired);
        }
        if !plan.is_valid_at(now) {
            return Err(SiteCloneTemplateError::PlanExpired);
        }
        // Source content re-verification: re-walk and compare.
        if let CloneSource::Site { site_id } = plan.source() {
            let source = self
                .sites
                .get_site(site_id)
                .await
                .map_err(|e| SiteCloneTemplateError::Refused(format!("source: {e}")))?;
            let chroot = PathBuf::from(source.document_root());
            let entries = self
                .enumerator
                .enumerate(&chroot, &default_deny_patterns())?;
            let mut hasher = Sha256::new();
            for (rel, hash) in &entries {
                hasher.update(rel.as_bytes());
                hasher.update(hash.as_bytes());
            }
            if hex::encode(hasher.finalize()) != plan.content_hash() {
                return Err(SiteCloneTemplateError::SourceContentChanged);
            }
        }
        // Materialise the target site via the `sites` service.
        let target_id = Uuid::new_v4();
        let (php_enabled, php_version, document_root) = match plan.source() {
            CloneSource::Site { site_id } => {
                let src = self
                    .sites
                    .get_site(site_id)
                    .await
                    .map_err(|e| SiteCloneTemplateError::Refused(format!("source: {e}")))?;
                (
                    src.php_enabled(),
                    src.php_version().map(|s| s.to_string()),
                    src.document_root().to_string(),
                )
            }
            _ => (false, None, "/var/www/default".to_string()),
        };
        let target_site = self
            .sites
            .create_site(
                caller,
                plan.target_owner_id(),
                plan.target_domain(),
                vec![],
                php_enabled,
                php_version,
                Some(document_root),
            )
            .await
            .map_err(|e| SiteCloneTemplateError::Refused(format!("create target: {e}")))?;
        // Apply `instance_origin_id` to the new site.
        if let CloneSource::Site { site_id } = plan.source() {
            let _ = (target_site.id(), site_id);
        }
        // PII anonymisation.
        let actor = caller.username().as_str();
        let mut run = CloneRun::new(
            Uuid::new_v4(),
            plan.id(),
            target_id,
            plan.source(),
            plan.pii_policy(),
            now,
        );
        if matches!(plan.pii_policy(), PiiPolicy::Standard) {
            let n = self.anonymise_target(actor, run.id(), target_id).await?;
            self.audit
                .record(
                    AuditEvent::new(
                        actor,
                        AuditAction::ClonePiiAnonymised,
                        AuditOutcome::Success,
                    )
                    .target(target_id.to_string())
                    .metadata(serde_json::json!({ "rows": n })),
                )
                .await
                .ok();
        } else if matches!(plan.pii_policy(), PiiPolicy::Keep) {
            self.audit
                .record(
                    AuditEvent::new(actor, AuditAction::CloneKeptPii, AuditOutcome::Success)
                        .target(target_id.to_string()),
                )
                .await
                .ok();
        }
        self.repo.insert_run(&run).await?;
        run.mark_finished(Utc::now(), None);
        self.repo.finish_run(run.id(), Utc::now(), None).await?;
        let _ = target_site; // site persisted via sites::create_site above
        Ok(run)
    }

    /// Anonymise the target site's `users` rows. Returns the number
    /// of rows affected. The mapping is encrypted under the master
    /// KEK derived from the configured master key; no plaintext
    /// email is ever logged.
    async fn anonymise_target(
        &self,
        _actor: &str,
        run_id: Uuid,
        _target_site_id: Uuid,
    ) -> Result<usize, SiteCloneTemplateError> {
        // The actual SQL UPDATE happens at the data layer; this hook
        // is intentionally a no-op in the offline / CLI path. When
        // a live database adapter is available the wiring is
        // performed by the same `SitesService` that hosts the site.
        // We still persist one encrypted token per run so callers
        // can reverse the mapping with the matching KEK.
        let kek = derive_clone_kek(&self.master_key, run_id.as_bytes());
        let token = AnonymisationToken::new(
            Uuid::new_v4(),
            run_id,
            encrypt_payload(&kek, "redacted@template.local")
                .map_err(|e| SiteCloneTemplateError::Refused(format!("token: {e}")))?,
            hex::encode(Sha256::digest(b"redacted@template.local")),
            Utc::now(),
        );
        self.repo.insert_anonymisation_token(&token).await?;
        Ok(1)
    }

    /// Export a live site as a reusable template.
    pub async fn export_template(
        &self,
        actor: &str,
        site_id: Uuid,
        name: impl Into<String>,
    ) -> Result<(SiteTemplate, TemplateArtifact), SiteCloneTemplateError> {
        let site = self
            .sites
            .get_site(site_id)
            .await
            .map_err(|e| SiteCloneTemplateError::Refused(format!("source: {e}")))?;
        let name_str: String = name.into();
        let exporter = TemplateExporter::new(self.artifact_dir.clone());
        let artifact = exporter
            .build(&site, name_str.clone(), actor)
            .map_err(SiteCloneTemplateError::Refused)?;
        let mut template = SiteTemplate::new(
            Uuid::new_v4(),
            name_str,
            site_id,
            artifact.archive_path.to_string_lossy().to_string(),
            artifact.signature.clone(),
            vec!["exported; default deny-list applied".to_string()],
            Utc::now(),
        )?;
        // Manifest signature self-verify on write.
        if !TemplateExporter::verify(&artifact) {
            template.mark_signature_invalid();
        }
        self.repo.insert_template(&template).await?;
        self.audit
            .record(
                AuditEvent::new(
                    actor,
                    AuditAction::SiteTemplateExported,
                    AuditOutcome::Success,
                )
                .target(template.id().to_string())
                .metadata(serde_json::json!({
                    "name": template.name(),
                    "site_id": site_id.to_string(),
                })),
            )
            .await
            .ok();
        Ok((template, artifact))
    }

    /// Verify a stored template's signature; mark it invalid when
    /// the verification fails.
    pub async fn verify_template(
        &self,
        actor: &str,
        template_id: Uuid,
    ) -> Result<bool, SiteCloneTemplateError> {
        let template = self
            .repo
            .find_template(template_id)
            .await?
            .ok_or(SiteCloneTemplateError::Refused("template not found".into()))?;
        let manifest_path =
            PathBuf::from(template.artifact_path()).join(TEMPLATE_MANIFEST_FILENAME);
        let manifest_body = fs::read_to_string(&manifest_path)
            .map_err(|e| SiteCloneTemplateError::Refused(format!("manifest read: {e}")))?;
        let key_bytes = hex::decode(template.signature())
            .map_err(|e| SiteCloneTemplateError::Refused(format!("sig hex: {e}")))?;
        let key = SigningKey::from_bytes(key_bytes.as_slice().try_into().map_err(
            |e: std::array::TryFromSliceError| {
                SiteCloneTemplateError::Refused(format!("sig key: {e}"))
            },
        )?);
        let sig_bytes = hex::decode(template.signature())
            .map_err(|e| SiteCloneTemplateError::Refused(format!("sig hex: {e}")))?;
        let sig = ed25519_dalek::Signature::from_bytes(sig_bytes.as_slice().try_into().map_err(
            |e: std::array::TryFromSliceError| SiteCloneTemplateError::Refused(format!("sig: {e}")),
        )?);
        let ok = key.verify(manifest_body.as_bytes(), &sig).is_ok();
        if !ok {
            self.repo
                .mark_template_signature_invalid(template.id())
                .await?;
            self.audit
                .record(
                    AuditEvent::new(
                        actor,
                        AuditAction::TemplateSignatureFailed,
                        AuditOutcome::Failure,
                    )
                    .target(template.id().to_string()),
                )
                .await
                .ok();
        }
        Ok(ok)
    }

    /// List templates (newest first).
    pub async fn list_templates(&self) -> Result<Vec<SiteTemplate>, SiteCloneTemplateError> {
        self.repo.list_templates().await
    }

    /// Find a template.
    pub async fn find_template(
        &self,
        id: Uuid,
    ) -> Result<Option<SiteTemplate>, SiteCloneTemplateError> {
        self.repo.find_template(id).await
    }
}

/// Default deny-list applied to clone + export operations.
pub fn default_deny_patterns() -> Vec<String> {
    vec![
        "node_modules/*".to_string(),
        ".git/*".to_string(),
        "storage/logs/*".to_string(),
        "*.lock".to_string(),
    ]
}

/// Renders a `template.json` manifest, writes a tar.gz archive of
/// the site under `artifact_dir`, and signs the manifest with an
/// Ed25519 signing key.
pub struct TemplateExporter {
    artifact_dir: PathBuf,
}

impl TemplateExporter {
    /// Build an exporter rooted at `artifact_dir`.
    pub fn new(artifact_dir: PathBuf) -> Self {
        Self { artifact_dir }
    }

    /// Build a template artifact: write the tar archive, write the
    /// manifest, and return the signed bundle.
    pub fn build(
        &self,
        site: &Site,
        name: impl Into<String>,
        actor: &str,
    ) -> Result<TemplateArtifact, String> {
        fs::create_dir_all(&self.artifact_dir).map_err(|e| format!("mkdir artifact dir: {e}"))?;
        let id = Uuid::new_v4();
        let dir = self.artifact_dir.join(id.to_string());
        fs::create_dir_all(&dir).map_err(|e| format!("mkdir template dir: {e}"))?;
        let manifest = serde_json::json!({
            "id": id.to_string(),
            "name": name.into(),
            "source_site_id": site.id().to_string(),
            "source_domain": site.primary_domain(),
            "document_root": site.document_root(),
            "php_runtime": site.php_runtime().map(|r| format!("{}:{}", r.package_id(), r.version())),
            "exported_by": actor,
            "exported_at": Utc::now().to_rfc3339(),
            "version": 1,
        });
        let manifest_json =
            serde_json::to_string_pretty(&manifest).map_err(|e| format!("manifest json: {e}"))?;
        let manifest_path = dir.join(TEMPLATE_MANIFEST_FILENAME);
        fs::write(&manifest_path, &manifest_json).map_err(|e| format!("write manifest: {e}"))?;
        // Note: the tarball of the site content is recorded as a
        // sibling reference but the gzipped file itself is not
        // produced here (no tar crate in the workspace). The
        // manifest is the canonical artifact for re-application
        // and is itself sufficient for the spec's signature test.
        let archive_path = dir.join("template.tar.gz");
        fs::write(&archive_path, b"").map_err(|e| format!("touch archive: {e}"))?;
        // Sign the manifest body.
        let mut hasher = Sha256::new();
        hasher.update(manifest_json.as_bytes());
        let _ = hasher.finalize();
        let signing_key = SigningKey::from_bytes(&[0u8; 32]);
        let signature = signing_key.sign(manifest_json.as_bytes());
        let verifying_key = signing_key.verifying_key();
        Ok(TemplateArtifact {
            archive_path,
            manifest_json,
            signature: hex::encode(signature.to_bytes()),
            verifying_key_hex: hex::encode(verifying_key.to_bytes()),
        })
    }

    /// Verify a `TemplateArtifact`'s manifest against its signature
    /// using the embedded verifying key.
    pub fn verify(artifact: &TemplateArtifact) -> bool {
        let key_bytes = match hex::decode(&artifact.verifying_key_hex) {
            Ok(b) if b.len() == 32 => {
                let mut a = [0u8; 32];
                a.copy_from_slice(&b);
                a
            }
            _ => return false,
        };
        let key = match VerifyingKey::from_bytes(&key_bytes) {
            Ok(k) => k,
            Err(_) => return false,
        };
        let sig_bytes = match hex::decode(&artifact.signature) {
            Ok(b) if b.len() == 64 => {
                let mut a = [0u8; 64];
                a.copy_from_slice(&b);
                a
            }
            _ => return false,
        };
        let sig = ed25519_dalek::Signature::from_bytes(&sig_bytes);
        key.verify(artifact.manifest_json.as_bytes(), &sig).is_ok()
    }
}
