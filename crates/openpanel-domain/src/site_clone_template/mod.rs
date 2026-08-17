//! Site clone and template export bounded context: the typed
//! `SiteTemplate`, `TemplateArtifact`, `ClonePlan`, `CloneRun`, and
//! `PiiPolicy` records plus the `CdnAdapter`-style repository port.
//!
//! I/O-free: the cloning service, template exporter, and signature
//! verification live in the application layer; this module only
//! encodes the invariants and the persistence contract.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::RepoError;

/// Where a clone is being sourced from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CloneSource {
    /// Clone from a live site id.
    Site {
        /// The source site id.
        site_id: Uuid,
    },
    /// Clone from a per-site snapshot id.
    Snapshot {
        /// The source site id.
        site_id: Uuid,
        /// The snapshot id.
        snapshot_id: i64,
    },
    /// Clone from a stored template.
    Template {
        /// The template id.
        template_id: Uuid,
    },
}

/// The PII handling policy for a clone.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PiiPolicy {
    /// Standard: replace `users.email` and rotate `password_hash`.
    #[default]
    Standard,
    /// Keep all PII verbatim; requires Owner role + fresh `confirmed_at`.
    Keep,
    /// Templates never carry PII; the policy is fixed.
    None,
}

impl PiiPolicy {
    /// Wire name.
    pub fn as_str(&self) -> &'static str {
        match self {
            PiiPolicy::Standard => "standard",
            PiiPolicy::Keep => "keep",
            PiiPolicy::None => "none",
        }
    }
}

/// A single file-copy entry inside a `ClonePlan`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CloneFile {
    /// Source path, relative to the source chroot.
    pub src: String,
    /// Destination path, relative to the target chroot.
    pub dst: String,
    /// Content hash (sha-256) at plan-time.
    pub content_hash: String,
    /// Whether the entry is skipped (deny-list match).
    pub skip: bool,
}

impl CloneFile {
    /// Build a file copy entry.
    pub fn new(
        src: impl Into<String>,
        dst: impl Into<String>,
        content_hash: impl Into<String>,
    ) -> Self {
        Self {
            src: src.into(),
            dst: dst.into(),
            content_hash: content_hash.into(),
            skip: false,
        }
    }
}

/// Database action summary inside a `ClonePlan`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "action")]
pub enum DbAction {
    /// Dump the source database and load it into a fresh target DB.
    DumpAndLoad {
        /// Estimated dump size in bytes.
        est_size_bytes: u64,
    },
    /// Import the schema with placeholder data (template source).
    ImportSchemaWithPlaceholderData,
    /// No database action (e.g. static-only site).
    None,
}

/// A pre-computed clone plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClonePlan {
    id: Uuid,
    source: CloneSource,
    target_owner_id: Uuid,
    target_domain: String,
    pii_policy: PiiPolicy,
    files: Vec<CloneFile>,
    db: DbAction,
    warnings: Vec<String>,
    content_hash: String,
    created_at: DateTime<Utc>,
    /// Plan expiry: a plan is valid for 5 minutes from `created_at`.
    expires_at: DateTime<Utc>,
}

impl ClonePlan {
    /// Maximum plan validity in seconds (5 minutes, per the spec).
    pub const MAX_VALIDITY_SECONDS: i64 = 300;

    /// Build a plan.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: Uuid,
        source: CloneSource,
        target_owner_id: Uuid,
        target_domain: impl Into<String>,
        pii_policy: PiiPolicy,
        files: Vec<CloneFile>,
        db: DbAction,
        warnings: Vec<String>,
        content_hash: impl Into<String>,
        created_at: DateTime<Utc>,
    ) -> Result<Self, SiteCloneTemplateError> {
        let target_domain = target_domain.into();
        if target_domain.is_empty() || target_domain.len() > 253 {
            return Err(SiteCloneTemplateError::InvalidTargetDomain(target_domain));
        }
        let expires_at = created_at + chrono::Duration::seconds(Self::MAX_VALIDITY_SECONDS);
        Ok(Self {
            id,
            source,
            target_owner_id,
            target_domain,
            pii_policy,
            files,
            db,
            warnings,
            content_hash: content_hash.into(),
            created_at,
            expires_at,
        })
    }

    /// Plan id.
    pub fn id(&self) -> Uuid {
        self.id
    }

    /// The source.
    pub fn source(&self) -> CloneSource {
        self.source
    }

    /// The target site owner.
    pub fn target_owner_id(&self) -> Uuid {
        self.target_owner_id
    }

    /// The target domain.
    pub fn target_domain(&self) -> &str {
        &self.target_domain
    }

    /// PII policy.
    pub fn pii_policy(&self) -> PiiPolicy {
        self.pii_policy
    }

    /// File copy entries.
    pub fn files(&self) -> &[CloneFile] {
        &self.files
    }

    /// Database action.
    pub fn db(&self) -> &DbAction {
        &self.db
    }

    /// Warnings recorded at plan time.
    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }

    /// SHA-256 content hash for run-time verification.
    pub fn content_hash(&self) -> &str {
        &self.content_hash
    }

    /// When the plan was created.
    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    /// When the plan expires.
    pub fn expires_at(&self) -> DateTime<Utc> {
        self.expires_at
    }

    /// Whether the plan is still valid at `at`.
    pub fn is_valid_at(&self, at: DateTime<Utc>) -> bool {
        at <= self.expires_at
    }
}

/// A confirmed, executed clone run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CloneRun {
    id: Uuid,
    plan_id: Uuid,
    target_site_id: Uuid,
    source: CloneSource,
    pii_policy: PiiPolicy,
    started_at: DateTime<Utc>,
    finished_at: Option<DateTime<Utc>>,
    failure_reason: Option<String>,
}

impl CloneRun {
    /// Build a new run.
    pub fn new(
        id: Uuid,
        plan_id: Uuid,
        target_site_id: Uuid,
        source: CloneSource,
        pii_policy: PiiPolicy,
        started_at: DateTime<Utc>,
    ) -> Self {
        Self {
            id,
            plan_id,
            target_site_id,
            source,
            pii_policy,
            started_at,
            finished_at: None,
            failure_reason: None,
        }
    }

    /// Plan id.
    pub fn plan_id(&self) -> Uuid {
        self.plan_id
    }

    /// Run id.
    pub fn id(&self) -> Uuid {
        self.id
    }

    /// Target site id.
    pub fn target_site_id(&self) -> Uuid {
        self.target_site_id
    }

    /// Source.
    pub fn source(&self) -> CloneSource {
        self.source
    }

    /// PII policy.
    pub fn pii_policy(&self) -> PiiPolicy {
        self.pii_policy
    }

    /// Started at.
    pub fn started_at(&self) -> DateTime<Utc> {
        self.started_at
    }

    /// Finished at.
    pub fn finished_at(&self) -> Option<DateTime<Utc>> {
        self.finished_at
    }

    /// Failure reason (when the run did not succeed).
    pub fn failure_reason(&self) -> Option<&str> {
        self.failure_reason.as_deref()
    }

    /// Mark the run finished.
    pub fn mark_finished(&mut self, at: DateTime<Utc>, failure_reason: Option<String>) {
        self.finished_at = Some(at);
        self.failure_reason = failure_reason;
    }
}

/// A stored, exportable site template.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SiteTemplate {
    id: Uuid,
    name: String,
    /// The source site id at export time (retained for lineage).
    source_site_id: Uuid,
    /// Path to the tar.gz artifact on disk.
    artifact_path: String,
    /// Ed25519 signature over the `template.json` body, hex-encoded.
    signature: String,
    /// Whether the artifact still verifies.
    signature_valid: bool,
    /// Warnings recorded during export.
    warnings: Vec<String>,
    /// PII policy baked into the template (always `None`).
    pii_policy: PiiPolicy,
    created_at: DateTime<Utc>,
}

impl SiteTemplate {
    /// Build a new template record.
    pub fn new(
        id: Uuid,
        name: impl Into<String>,
        source_site_id: Uuid,
        artifact_path: impl Into<String>,
        signature: impl Into<String>,
        warnings: Vec<String>,
        created_at: DateTime<Utc>,
    ) -> Result<Self, SiteCloneTemplateError> {
        let name = name.into();
        if name.is_empty() || name.len() > 128 {
            return Err(SiteCloneTemplateError::InvalidTemplateName(name));
        }
        Ok(Self {
            id,
            name,
            source_site_id,
            artifact_path: artifact_path.into(),
            signature: signature.into(),
            signature_valid: true,
            warnings,
            pii_policy: PiiPolicy::None,
            created_at,
        })
    }

    /// Template id.
    pub fn id(&self) -> Uuid {
        self.id
    }

    /// Template display name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Source site id.
    pub fn source_site_id(&self) -> Uuid {
        self.source_site_id
    }

    /// Artifact path on disk.
    pub fn artifact_path(&self) -> &str {
        &self.artifact_path
    }

    /// Signature.
    pub fn signature(&self) -> &str {
        &self.signature
    }

    /// Whether the stored signature still verifies.
    pub fn signature_valid(&self) -> bool {
        self.signature_valid
    }

    /// Mark the signature invalid (after a verification failure).
    pub fn mark_signature_invalid(&mut self) {
        self.signature_valid = false;
    }

    /// Warnings recorded at export.
    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }

    /// PII policy (always `None` for templates).
    pub fn pii_policy(&self) -> PiiPolicy {
        self.pii_policy
    }

    /// Created at.
    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
}

/// An encrypted anonymisation token: a `(original_email, new_email)`
/// mapping encrypted under the master key, reverseable only with
/// the corresponding KEK.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnonymisationToken {
    id: Uuid,
    /// The run id this token belongs to.
    run_id: Uuid,
    /// The encrypted blob (`hex(nonce) || hex(ciphertext)`).
    cipher_text: String,
    /// SHA-256 of the original email (used to find the row).
    original_hash: String,
    created_at: DateTime<Utc>,
}

impl AnonymisationToken {
    /// Build a token.
    pub fn new(
        id: Uuid,
        run_id: Uuid,
        cipher_text: impl Into<String>,
        original_hash: impl Into<String>,
        created_at: DateTime<Utc>,
    ) -> Self {
        Self {
            id,
            run_id,
            cipher_text: cipher_text.into(),
            original_hash: original_hash.into(),
            created_at,
        }
    }

    /// Token id.
    pub fn id(&self) -> Uuid {
        self.id
    }

    /// Run id.
    pub fn run_id(&self) -> Uuid {
        self.run_id
    }

    /// Cipher text.
    pub fn cipher_text(&self) -> &str {
        &self.cipher_text
    }

    /// SHA-256 of the original email.
    pub fn original_hash(&self) -> &str {
        &self.original_hash
    }

    /// Created at.
    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
}

/// Repository port for the site-clone-template bounded context.
#[async_trait]
pub trait SiteCloneTemplateRepository: Send + Sync + 'static {
    /// Persist a plan.
    async fn upsert_plan(&self, plan: &ClonePlan) -> Result<(), SiteCloneTemplateError>;
    /// Load a plan by id.
    async fn find_plan(&self, id: Uuid) -> Result<Option<ClonePlan>, SiteCloneTemplateError>;
    /// Persist a run.
    async fn insert_run(&self, run: &CloneRun) -> Result<(), SiteCloneTemplateError>;
    /// Update a run's finish state.
    async fn finish_run(
        &self,
        run_id: Uuid,
        finished_at: DateTime<Utc>,
        failure_reason: Option<String>,
    ) -> Result<(), SiteCloneTemplateError>;
    /// Persist a template.
    async fn insert_template(&self, template: &SiteTemplate) -> Result<(), SiteCloneTemplateError>;
    /// Find a template by id.
    async fn find_template(&self, id: Uuid)
    -> Result<Option<SiteTemplate>, SiteCloneTemplateError>;
    /// List all templates (newest first).
    async fn list_templates(&self) -> Result<Vec<SiteTemplate>, SiteCloneTemplateError>;
    /// Mark a template's signature invalid.
    async fn mark_template_signature_invalid(&self, id: Uuid)
    -> Result<(), SiteCloneTemplateError>;
    /// Persist an anonymisation token.
    async fn insert_anonymisation_token(
        &self,
        token: &AnonymisationToken,
    ) -> Result<(), SiteCloneTemplateError>;
}

/// Errors that can occur in the site-clone-template bounded context.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum SiteCloneTemplateError {
    /// The target domain is invalid.
    #[error("invalid target domain: {0}")]
    InvalidTargetDomain(String),
    /// The template name is invalid.
    #[error("invalid template name: {0}")]
    InvalidTemplateName(String),
    /// The plan has expired.
    #[error("clone plan has expired")]
    PlanExpired,
    /// The source content hash no longer matches.
    #[error("source content has changed since plan")]
    SourceContentChanged,
    /// The template signature did not verify.
    #[error("template signature failed to verify")]
    TemplateSignatureFailed,
    /// The chroot was violated.
    #[error("path escapes the site chroot: {0}")]
    OutsideChroot(String),
    /// The clone was refused.
    #[error("clone refused: {0}")]
    Refused(String),
    /// Persistence layer failure.
    #[error("clone-template persistence error: {0}")]
    Persistence(String),
}

impl From<RepoError> for SiteCloneTemplateError {
    fn from(error: RepoError) -> Self {
        SiteCloneTemplateError::Persistence(error.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> DateTime<Utc> {
        Utc::now()
    }

    #[test]
    fn plan_validity_is_5_minutes() {
        let plan = ClonePlan::new(
            Uuid::new_v4(),
            CloneSource::Site {
                site_id: Uuid::new_v4(),
            },
            Uuid::new_v4(),
            "staging.example.com",
            PiiPolicy::Standard,
            vec![],
            DbAction::None,
            vec![],
            "abc",
            now(),
        )
        .expect("plan");
        let future = plan.expires_at() + chrono::Duration::seconds(1);
        assert!(!plan.is_valid_at(future));
        assert!(plan.is_valid_at(plan.created_at()));
    }

    #[test]
    fn plan_rejects_empty_target_domain() {
        let err = ClonePlan::new(
            Uuid::new_v4(),
            CloneSource::Site {
                site_id: Uuid::new_v4(),
            },
            Uuid::new_v4(),
            "",
            PiiPolicy::Standard,
            vec![],
            DbAction::None,
            vec![],
            "abc",
            now(),
        )
        .expect_err("empty domain");
        assert!(matches!(
            err,
            SiteCloneTemplateError::InvalidTargetDomain(_)
        ));
    }

    #[test]
    fn template_rejects_empty_name() {
        let err = SiteTemplate::new(
            Uuid::new_v4(),
            "",
            Uuid::new_v4(),
            "/tmp/x.tar.gz",
            "deadbeef",
            vec![],
            now(),
        )
        .expect_err("empty name");
        assert!(matches!(
            err,
            SiteCloneTemplateError::InvalidTemplateName(_)
        ));
    }

    #[test]
    fn template_marks_signature_invalid() {
        let mut t = SiteTemplate::new(
            Uuid::new_v4(),
            "name",
            Uuid::new_v4(),
            "/tmp/x.tar.gz",
            "deadbeef",
            vec![],
            now(),
        )
        .expect("t");
        assert!(t.signature_valid());
        t.mark_signature_invalid();
        assert!(!t.signature_valid());
    }

    #[test]
    fn pii_policy_wire_names() {
        assert_eq!(PiiPolicy::Standard.as_str(), "standard");
        assert_eq!(PiiPolicy::Keep.as_str(), "keep");
        assert_eq!(PiiPolicy::None.as_str(), "none");
    }
}
