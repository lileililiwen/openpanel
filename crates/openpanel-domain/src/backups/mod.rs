//! Backup and restore domain model.

use std::path::{Component, Path, PathBuf};

use chrono::{DateTime, Utc};
// Re-export the resource-scoped restore + remote-target policy
// refinement types from the `refine-backups-with-resource-restore-and-policies`
// change so callers can import them from the `backups` module.
pub use refine::{
    BackupRefineError, BackupTargetKind, BackupTargetPolicy, ResourceKind, RestoreRequest,
    RestoreScope,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;

use crate::cron::CronSchedule;

pub mod drill;
pub mod health;
pub mod migration;
mod refine;
pub mod snapshot;

/// Shared operator-guidance accessor for backup summaries.
///
/// One trait instead of three same-named inherent methods: health,
/// migration-readiness, and remote-verification reports all carry
/// safe operator guidance, and callers use one vocabulary for it.
pub trait Guidance {
    /// Safe operator guidance (links to logs/retry/configuration).
    fn guidance(&self) -> &str;
}

/// Domain validation and transition failures.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum BackupError {
    /// A value is invalid.
    #[error("invalid backup value: {0}")]
    Invalid(String),
    /// The requested lifecycle transition is not allowed.
    #[error("invalid backup transition")]
    InvalidTransition,
    /// The manifest format is newer than this binary supports.
    #[error("unsupported backup format version {0}")]
    UnsupportedFormat(u32),
}

/// A resource selected for capture or restore.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
pub enum BackupResource {
    /// Hosted site files.
    Site(Uuid),
    /// Managed database dump.
    Database(Uuid),
    /// Non-secret OpenPanel metadata.
    PanelMetadata,
}

/// Copy-count retention policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetentionPolicy {
    copies: usize,
}

impl RetentionPolicy {
    /// Retain the newest `copies` unpinned completed runs.
    pub fn copies(copies: usize) -> Result<Self, BackupError> {
        if copies == 0 {
            Err(BackupError::Invalid(
                "retention copies must be positive".into(),
            ))
        } else {
            Ok(Self { copies })
        }
    }

    /// Select completed, unpinned runs outside the policy.
    pub fn deletions(&self, candidates: &[RetentionCandidate]) -> Vec<Uuid> {
        let mut eligible: Vec<_> = candidates
            .iter()
            .filter(|run| run.completed && !run.pinned)
            .collect();
        eligible.sort_by_key(|run| std::cmp::Reverse(run.sequence));
        eligible
            .into_iter()
            .skip(self.copies)
            .map(|run| run.id)
            .collect()
    }

    /// Configured copy count.
    pub fn count(&self) -> usize {
        self.copies
    }
}

/// Minimal retention projection.
#[derive(Debug, Clone, Copy)]
pub struct RetentionCandidate {
    id: Uuid,
    sequence: i64,
    completed: bool,
    pinned: bool,
}
impl RetentionCandidate {
    /// A terminal candidate.
    pub fn completed(id: Uuid, sequence: i64, pinned: bool) -> Self {
        Self {
            id,
            sequence,
            completed: true,
            pinned,
        }
    }

    /// An active run, which is never deleted.
    pub fn active(id: Uuid, sequence: i64) -> Self {
        Self {
            id,
            sequence,
            completed: false,
            pinned: false,
        }
    }

    /// Candidate id.
    pub fn id(&self) -> Uuid {
        self.id
    }
}

/// Persistent recurring backup selection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupPlan {
    id: Uuid,
    owner_id: Uuid,
    name: String,
    resources: Vec<BackupResource>,
    schedule: String,
    timezone: String,
    retention: RetentionPolicy,
    enabled: bool,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    #[serde(default)]
    cron_job_id: Option<Uuid>,
}

impl BackupPlan {
    /// Create an enabled, validated plan.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: Uuid,
        owner_id: Uuid,
        name: impl Into<String>,
        resources: Vec<BackupResource>,
        schedule: impl Into<String>,
        timezone: impl Into<String>,
        retention: RetentionPolicy,
        now: DateTime<Utc>,
    ) -> Result<Self, BackupError> {
        let name = name.into();
        let schedule = schedule.into();
        let timezone = timezone.into();
        if name.trim().is_empty() || resources.is_empty() {
            return Err(BackupError::Invalid(
                "name and resources are required".into(),
            ));
        }
        CronSchedule::parse(schedule.clone(), timezone.clone())
            .map_err(|e| BackupError::Invalid(e.to_string()))?;
        Ok(Self {
            id,
            owner_id,
            name,
            resources,
            schedule,
            timezone,
            retention,
            enabled: true,
            created_at: now,
            updated_at: now,
            cron_job_id: None,
        })
    }

    /// Disable scheduled capture.
    pub fn disable(&mut self) {
        self.enabled = false;
        self.updated_at = Utc::now();
    }

    /// Enable scheduled capture.
    pub fn enable(&mut self) {
        self.enabled = true;
        self.updated_at = Utc::now();
    }

    /// Whether scheduled capture is enabled.
    pub fn enabled(&self) -> bool {
        self.enabled
    }

    /// Plan id.
    pub fn id(&self) -> Uuid {
        self.id
    }

    /// Owner id.
    pub fn owner_id(&self) -> Uuid {
        self.owner_id
    }

    /// Display name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Selected resources.
    pub fn resources(&self) -> &[BackupResource] {
        &self.resources
    }

    /// Schedule expression.
    pub fn schedule(&self) -> &str {
        &self.schedule
    }

    /// Schedule timezone.
    pub fn timezone(&self) -> &str {
        &self.timezone
    }

    /// Retention policy.
    pub fn retention(&self) -> RetentionPolicy {
        self.retention
    }

    /// Link this plan to the shared cron scheduler job.
    pub fn link_cron(&mut self, cron_job_id: Uuid, now: DateTime<Utc>) {
        self.cron_job_id = Some(cron_job_id);
        self.updated_at = now;
    }

    /// Shared cron scheduler job id, once linked.
    pub fn cron_job_id(&self) -> Option<Uuid> {
        self.cron_job_id
    }

    /// Update mutable plan fields.
    pub fn update(
        &mut self,
        name: Option<String>,
        retention: Option<RetentionPolicy>,
        now: DateTime<Utc>,
    ) -> Result<(), BackupError> {
        if let Some(name) = name {
            if name.trim().is_empty() {
                return Err(BackupError::Invalid("name is required".into()));
            }
            self.name = name;
        }
        if let Some(retention) = retention {
            self.retention = retention;
        }
        self.updated_at = now;
        Ok(())
    }

    /// Serialize for repository storage.
    pub fn to_json(&self) -> Result<String, BackupError> {
        serde_json::to_string(self).map_err(|e| BackupError::Invalid(e.to_string()))
    }

    /// Restore repository storage.
    pub fn from_json(json: &str) -> Result<Self, BackupError> {
        serde_json::from_str(json).map_err(|e| BackupError::Invalid(e.to_string()))
    }
}

/// One streamed and checksummed artifact descriptor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupArtifact {
    resource: BackupResource,
    path: String,
    size: u64,
    sha256: String,
    format_version: u32,
}

impl BackupArtifact {
    /// Describe captured bytes without retaining secret-bearing content.
    pub fn new(
        resource: BackupResource,
        path: impl Into<String>,
        content: &[u8],
    ) -> Result<Self, BackupError> {
        let path = RestorePath::new(path.into())?;
        Ok(Self {
            resource,
            path: path.as_str().into(),
            size: content.len() as u64,
            sha256: hex::encode(Sha256::digest(content)),
            format_version: 1,
        })
    }

    /// Describe a streamed artifact from its final size and SHA-256.
    pub fn from_digest(
        resource: BackupResource,
        path: impl Into<String>,
        size: u64,
        sha256: impl Into<String>,
    ) -> Result<Self, BackupError> {
        let path = RestorePath::new(path.into())?;
        let sha256 = sha256.into();
        if sha256.len() != 64 || !sha256.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(BackupError::Invalid("invalid SHA-256".into()));
        }
        Ok(Self {
            resource,
            path: path.as_str().into(),
            size,
            sha256: sha256.to_lowercase(),
            format_version: 1,
        })
    }

    /// Verify bytes against the recorded size and SHA-256.
    pub fn verify(&self, content: &[u8]) -> bool {
        self.size == content.len() as u64 && self.sha256 == hex::encode(Sha256::digest(content))
    }

    /// Relative artifact path.
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Selected resource.
    pub fn resource(&self) -> &BackupResource {
        &self.resource
    }

    /// Recorded byte size.
    pub fn size(&self) -> u64 {
        self.size
    }

    /// Recorded checksum.
    pub fn sha256(&self) -> &str {
        &self.sha256
    }
}

/// Current versioned backup manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupManifest {
    format_version: u32,
    run_id: Uuid,
    artifacts: Vec<BackupArtifact>,
    created_at: DateTime<Utc>,
}

impl BackupManifest {
    /// Build a version-one non-empty manifest.
    pub fn new(
        run_id: Uuid,
        artifacts: Vec<BackupArtifact>,
        created_at: DateTime<Utc>,
    ) -> Result<Self, BackupError> {
        if artifacts.is_empty() {
            return Err(BackupError::Invalid(
                "manifest must contain artifacts".into(),
            ));
        }
        Ok(Self {
            format_version: 1,
            run_id,
            artifacts,
            created_at,
        })
    }

    /// Serialize for durable storage.
    pub fn to_json(&self) -> Result<String, BackupError> {
        serde_json::to_string(self).map_err(|e| BackupError::Invalid(e.to_string()))
    }

    /// Parse only supported versions.
    pub fn from_json(json: &str) -> Result<Self, BackupError> {
        let manifest: Self =
            serde_json::from_str(json).map_err(|e| BackupError::Invalid(e.to_string()))?;
        if manifest.format_version != 1 {
            return Err(BackupError::UnsupportedFormat(manifest.format_version));
        }
        Ok(manifest)
    }

    /// Artifact descriptors.
    pub fn artifacts(&self) -> &[BackupArtifact] {
        &self.artifacts
    }

    /// Run id.
    pub fn run_id(&self) -> Uuid {
        self.run_id
    }
}

/// Durable backup run state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackupRunState {
    /// Awaiting worker pickup.
    Pending,
    /// Capturing resources.
    Running,
    /// Verified and atomically finalized.
    Completed,
    /// Capture or finalization failed.
    Failed,
    /// Explicitly cancelled.
    Cancelled,
    /// Checksum verification failed.
    Corrupt,
}

/// Backup execution aggregate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupRun {
    id: Uuid,
    plan_id: Uuid,
    owner_id: Uuid,
    state: BackupRunState,
    artifacts: Vec<BackupArtifact>,
    finalized: bool,
    pinned: bool,
    created_at: DateTime<Utc>,
    started_at: Option<DateTime<Utc>>,
    finished_at: Option<DateTime<Utc>>,
    error: Option<String>,
}

impl BackupRun {
    /// Create a pending run.
    pub fn new(id: Uuid, plan_id: Uuid, owner_id: Uuid, now: DateTime<Utc>) -> Self {
        Self {
            id,
            plan_id,
            owner_id,
            state: BackupRunState::Pending,
            artifacts: vec![],
            finalized: false,
            pinned: false,
            created_at: now,
            started_at: None,
            finished_at: None,
            error: None,
        }
    }

    /// Begin capture.
    pub fn start(&mut self, now: DateTime<Utc>) -> Result<(), BackupError> {
        if self.state != BackupRunState::Pending {
            return Err(BackupError::InvalidTransition);
        }
        self.state = BackupRunState::Running;
        self.started_at = Some(now);
        Ok(())
    }

    /// Attach one captured artifact while running.
    pub fn add_artifact(&mut self, artifact: BackupArtifact) -> Result<(), BackupError> {
        if self.state != BackupRunState::Running || self.finalized {
            return Err(BackupError::InvalidTransition);
        }
        self.artifacts.push(artifact);
        Ok(())
    }

    /// Record atomic destination finalization.
    pub fn mark_finalized(&mut self) -> Result<(), BackupError> {
        if self.state != BackupRunState::Running || self.artifacts.is_empty() {
            return Err(BackupError::InvalidTransition);
        }
        self.finalized = true;
        Ok(())
    }

    /// Complete only a finalized run.
    pub fn complete(&mut self, now: DateTime<Utc>) -> Result<(), BackupError> {
        if self.state != BackupRunState::Running || !self.finalized {
            return Err(BackupError::InvalidTransition);
        }
        self.state = BackupRunState::Completed;
        self.finished_at = Some(now);
        Ok(())
    }

    /// Fail an active run with a redacted reason.
    pub fn fail(
        &mut self,
        now: DateTime<Utc>,
        reason: impl Into<String>,
    ) -> Result<(), BackupError> {
        if !matches!(
            self.state,
            BackupRunState::Pending | BackupRunState::Running
        ) {
            return Err(BackupError::InvalidTransition);
        }
        self.state = BackupRunState::Failed;
        self.finished_at = Some(now);
        self.error = Some(reason.into());
        Ok(())
    }

    /// Cancel an active run.
    pub fn cancel(&mut self, now: DateTime<Utc>) -> Result<(), BackupError> {
        if !matches!(
            self.state,
            BackupRunState::Pending | BackupRunState::Running
        ) {
            return Err(BackupError::InvalidTransition);
        }
        self.state = BackupRunState::Cancelled;
        self.finished_at = Some(now);
        Ok(())
    }

    /// Mark a completed run corrupt.
    pub fn mark_corrupt(&mut self, now: DateTime<Utc>) -> Result<(), BackupError> {
        if self.state != BackupRunState::Completed {
            return Err(BackupError::InvalidTransition);
        }
        self.state = BackupRunState::Corrupt;
        self.finished_at = Some(now);
        Ok(())
    }

    /// Current state.
    pub fn state(&self) -> BackupRunState {
        self.state
    }

    /// Run id.
    pub fn id(&self) -> Uuid {
        self.id
    }

    /// Plan id.
    pub fn plan_id(&self) -> Uuid {
        self.plan_id
    }

    /// Owner id.
    pub fn owner_id(&self) -> Uuid {
        self.owner_id
    }

    /// Artifacts.
    pub fn artifacts(&self) -> &[BackupArtifact] {
        &self.artifacts
    }

    /// Whether retention must preserve this run.
    pub fn pinned(&self) -> bool {
        self.pinned
    }

    /// Serialize for repository storage.
    pub fn to_json(&self) -> Result<String, BackupError> {
        serde_json::to_string(self).map_err(|e| BackupError::Invalid(e.to_string()))
    }

    /// Restore repository storage.
    pub fn from_json(json: &str) -> Result<Self, BackupError> {
        serde_json::from_str(json).map_err(|e| BackupError::Invalid(e.to_string()))
    }
}

/// A validated relative archive/restore path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RestorePath(PathBuf);
impl RestorePath {
    /// Reject absolute, parent-traversing, and empty paths.
    pub fn new(path: impl AsRef<Path>) -> Result<Self, BackupError> {
        let path = path.as_ref();
        if path.as_os_str().is_empty()
            || path.is_absolute()
            || path.components().any(|part| {
                matches!(
                    part,
                    Component::ParentDir | Component::RootDir | Component::Prefix(_)
                )
            })
        {
            return Err(BackupError::Invalid("unsafe restore path".into()));
        }
        Ok(Self(path.to_path_buf()))
    }

    /// UTF-8 path text.
    pub fn as_str(&self) -> &str {
        self.0.to_str().unwrap_or("")
    }
}

/// Short-lived explicit overwrite confirmation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OverwriteConfirmation(String);
impl OverwriteConfirmation {
    /// Validate a non-trivial opaque token.
    pub fn new(token: impl Into<String>) -> Result<Self, BackupError> {
        let token = token.into();
        if token.len() < 12 {
            Err(BackupError::Invalid(
                "overwrite confirmation is invalid".into(),
            ))
        } else {
            Ok(Self(token))
        }
    }
}

/// Restore conflict behavior.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ConflictPolicy {
    /// Abort without changing targets.
    #[default]
    Fail,
    /// Replace existing targets after confirmation.
    Overwrite(OverwriteConfirmation),
}
impl ConflictPolicy {
    /// Build overwrite policy only with a confirmation.
    pub fn overwrite(confirmation: Option<OverwriteConfirmation>) -> Result<Self, BackupError> {
        confirmation
            .map(Self::Overwrite)
            .ok_or_else(|| BackupError::Invalid("overwrite requires confirmation".into()))
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use proptest::prelude::*;
    use uuid::Uuid;

    use super::*;

    fn resources() -> Vec<BackupResource> {
        vec![
            BackupResource::Site(Uuid::new_v4()),
            BackupResource::PanelMetadata,
        ]
    }

    #[test]
    fn plan_requires_name_resources_schedule_and_positive_retention() {
        let now = Utc::now();
        assert!(
            BackupPlan::new(
                Uuid::new_v4(),
                Uuid::new_v4(),
                "",
                resources(),
                "0 2 * * *",
                "UTC",
                RetentionPolicy::copies(3).unwrap(),
                now
            )
            .is_err()
        );
        assert!(
            BackupPlan::new(
                Uuid::new_v4(),
                Uuid::new_v4(),
                "nightly",
                vec![],
                "0 2 * * *",
                "UTC",
                RetentionPolicy::copies(3).unwrap(),
                now
            )
            .is_err()
        );
        assert!(RetentionPolicy::copies(0).is_err());
        let mut plan = BackupPlan::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            "nightly",
            resources(),
            "0 2 * * *",
            "Asia/Shanghai",
            RetentionPolicy::copies(3).unwrap(),
            now,
        )
        .unwrap();
        assert!(plan.enabled());
        plan.disable();
        assert!(!plan.enabled());
        plan.enable();
        assert!(plan.enabled());
    }

    #[test]
    fn run_only_completes_after_verified_atomic_finalization() {
        let now = Utc::now();
        let mut run = BackupRun::new(Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4(), now);
        run.start(now).unwrap();
        let artifact = BackupArtifact::new(
            BackupResource::PanelMetadata,
            "panel.json",
            b"safe metadata",
        )
        .unwrap();
        run.add_artifact(artifact).unwrap();
        assert!(run.complete(now).is_err());
        run.mark_finalized().unwrap();
        run.complete(now).unwrap();
        assert_eq!(run.state(), BackupRunState::Completed);
        assert!(run.cancel(now).is_err());
    }

    #[test]
    fn checksum_tampering_is_detected_and_blocks_restore() {
        let artifact =
            BackupArtifact::new(BackupResource::PanelMetadata, "panel.json", b"original").unwrap();
        assert!(artifact.verify(b"original"));
        assert!(!artifact.verify(b"tampered"));
    }

    #[test]
    fn manifest_round_trips_and_rejects_future_format() {
        let artifact =
            BackupArtifact::new(BackupResource::PanelMetadata, "panel.json", b"metadata").unwrap();
        let manifest = BackupManifest::new(Uuid::new_v4(), vec![artifact], Utc::now()).unwrap();
        let json = manifest.to_json().unwrap();
        assert_eq!(BackupManifest::from_json(&json).unwrap(), manifest);
        let future = json.replace("\"format_version\":1", "\"format_version\":999");
        assert!(BackupManifest::from_json(&future).is_err());
    }

    #[test]
    fn restore_paths_and_conflicts_are_safe_by_default() {
        assert!(RestorePath::new("sites/example/public_html/index.html").is_ok());
        for unsafe_path in ["/etc/passwd", "../escape", "sites/../../escape"] {
            assert!(RestorePath::new(unsafe_path).is_err());
        }
        assert_eq!(ConflictPolicy::default(), ConflictPolicy::Fail);
        assert!(ConflictPolicy::overwrite(None).is_err());
        assert!(
            ConflictPolicy::overwrite(Some(
                OverwriteConfirmation::new("short-lived-token").unwrap()
            ))
            .is_ok()
        );
    }

    #[test]
    fn retention_preserves_active_and_pinned_runs() {
        let plan = RetentionPolicy::copies(2).unwrap();
        let runs = vec![
            RetentionCandidate::completed(Uuid::new_v4(), 1, false),
            RetentionCandidate::completed(Uuid::new_v4(), 2, true),
            RetentionCandidate::completed(Uuid::new_v4(), 3, false),
            RetentionCandidate::active(Uuid::new_v4(), 0),
            RetentionCandidate::completed(Uuid::new_v4(), 4, false),
        ];
        let deletions = plan.deletions(&runs);
        assert_eq!(deletions, vec![runs[0].id()]);
    }

    proptest! {
        #[test]
        fn prop_manifest_json_round_trip(payload in proptest::collection::vec(any::<u8>(), 0..4096)) {
            let artifact = BackupArtifact::new(BackupResource::PanelMetadata, "panel.bin", &payload).unwrap();
            let manifest = BackupManifest::new(Uuid::new_v4(), vec![artifact], Utc::now()).unwrap();
            prop_assert_eq!(BackupManifest::from_json(&manifest.to_json().unwrap()).unwrap(), manifest);
        }

        #[test]
        fn prop_any_changed_byte_fails_checksum(payload in proptest::collection::vec(any::<u8>(), 1..2048), index in 0usize..2048) {
            let artifact = BackupArtifact::new(BackupResource::PanelMetadata, "panel.bin", &payload).unwrap();
            let mut changed = payload.clone(); let i = index % changed.len(); changed[i] ^= 0xff;
            prop_assert!(!artifact.verify(&changed));
        }

        #[test]
        fn prop_parent_restore_paths_are_rejected(prefix in "[A-Za-z0-9_-]{0,20}", suffix in "[A-Za-z0-9_-]{0,20}") {
            let path = format!("{}/../{}/../../escape", prefix, suffix);
            prop_assert!(RestorePath::new(path).is_err());
        }
    }
}
