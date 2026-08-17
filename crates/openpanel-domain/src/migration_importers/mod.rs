//! Migration importers bounded context: the `MigrationDriver`
//! contract, `MigrationPlan` preview results, `ImportedResource`
//! outcomes, and the redacted `TranslationLog` that the import
//! run persists.
//!
//! The format sniffing (`sniff`) and the async preview/run/rollback
//! lifecycle are declared as traits here; the concrete cPanel /
//! Baota / tar manifest drivers live in the application layer where
//! tar and gzip access is available.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::RepoError;

/// Stable identifier for a migration plan (the preview output).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MigrationPlanId(pub Uuid);

impl MigrationPlanId {
    /// Brand a uuid as a plan id.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Underlying uuid.
    pub fn as_uuid(&self) -> Uuid {
        self.0
    }
}

impl Default for MigrationPlanId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for MigrationPlanId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Stable identifier for an import run (the committed import).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MigrationRunId(pub Uuid);

impl MigrationRunId {
    /// Brand a uuid as a run id.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Underlying uuid.
    pub fn as_uuid(&self) -> Uuid {
        self.0
    }
}

impl Default for MigrationRunId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for MigrationRunId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// The backup formats an importer can recognize and translate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DriverKind {
    /// cPanel `pkgacct` tar format.
    CpanelPkgacct,
    /// cPanel `legacy` backup format predating `pkgacct`.
    CpanelLegacyBackup,
    /// Baota `/www/backup/` bundle format.
    BaotaBackup,
    /// OpenPanel's own `tar-with-json-manifest` format.
    TarWithJsonManifest,
}

impl DriverKind {
    /// Wire name used by the `/migration/import/preview`
    /// `driver_hint` field and the CLI `--driver` flag.
    pub fn as_str(&self) -> &'static str {
        match self {
            DriverKind::CpanelPkgacct => "cpanel-pkgacct",
            DriverKind::CpanelLegacyBackup => "cpanel-legacy-backup",
            DriverKind::BaotaBackup => "baota-backup",
            DriverKind::TarWithJsonManifest => "tar-with-json-manifest",
        }
    }
}

impl std::fmt::Display for DriverKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The kind of resource a migration can import. Mirrors the
/// per-bounded-context resource vocabulary used by the import
/// translators (identity, sites, databases, mail, dns, cron).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportedResourceKind {
    /// A user account (cPanel `pkgacct` users → identity).
    User,
    /// A website / vhost.
    Site,
    /// A managed database.
    Database,
    /// A mail domain (mailboxes + aliases).
    MailDomain,
    /// A single mailbox.
    Mailbox,
    /// A DNS zone.
    DnsZone,
    /// A cron job.
    CronJob,
    /// An SSL certificate bundle.
    SslCertificate,
}

/// One resource discovered by the preview driver.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlannedResource {
    /// Kind of the resource.
    pub kind: ImportedResourceKind,
    /// Natural key inside the source (domain name, username, …).
    pub source_key: String,
    /// Human-readable description surfaced in the preview.
    pub description: String,
    /// Estimated payload bytes (from the tar listing).
    pub bytes: u64,
}

impl PlannedResource {
    /// Build a planned resource. The source key must be non-empty
    /// and no longer than 255 chars.
    pub fn new(
        kind: ImportedResourceKind,
        source_key: impl Into<String>,
        description: impl Into<String>,
        bytes: u64,
    ) -> Result<Self, MigrationError> {
        let source_key = source_key.into();
        let description = description.into();
        if source_key.is_empty() || source_key.len() > 255 {
            return Err(MigrationError::InvalidSourceKey(format!(
                "source key must be 1..=255 chars, got {}",
                source_key.len()
            )));
        }
        Ok(Self {
            kind,
            source_key,
            description,
            bytes,
        })
    }
}

/// A conflict detected during preview: the target already owns
/// the resource under the natural key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportConflict {
    /// Kind of the conflicting resource.
    pub kind: ImportedResourceKind,
    /// Natural key that collides on the target.
    pub source_key: String,
    /// Why the conflict blocks (or warns) the import.
    pub reason: String,
}

/// A non-blocking observation made during preview or run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MigrationWarning {
    /// Human-readable warning. Diagnotics are redacted by the
    /// translation log before it is persisted.
    pub message: String,
}

/// The result of a `dry_run`: everything the operator needs to
/// decide whether to confirm the import.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MigrationPlan {
    plan_id: MigrationPlanId,
    driver: DriverKind,
    resources: Vec<PlannedResource>,
    conflicts: Vec<ImportConflict>,
    warnings: Vec<MigrationWarning>,
    created_at: DateTime<Utc>,
}

impl MigrationPlan {
    /// Build a plan from a driver's dry-run output.
    pub fn new(
        plan_id: MigrationPlanId,
        driver: DriverKind,
        resources: Vec<PlannedResource>,
        conflicts: Vec<ImportConflict>,
        warnings: Vec<MigrationWarning>,
        created_at: DateTime<Utc>,
    ) -> Self {
        Self {
            plan_id,
            driver,
            resources,
            conflicts,
            warnings,
            created_at,
        }
    }

    /// Identifier.
    pub fn plan_id(&self) -> MigrationPlanId {
        self.plan_id
    }

    /// Driver that produced the plan.
    pub fn driver(&self) -> DriverKind {
        self.driver
    }

    /// Resources proposed for import.
    pub fn resources(&self) -> &[PlannedResource] {
        &self.resources
    }

    /// Resources that collide with the target.
    pub fn conflicts(&self) -> &[ImportConflict] {
        &self.conflicts
    }

    /// Non-blocking observations.
    pub fn warnings(&self) -> &[MigrationWarning] {
        &self.warnings
    }

    /// When the plan was created.
    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    /// Total planned bytes across all resources.
    pub fn total_bytes(&self) -> u64 {
        self.resources.iter().map(|r| r.bytes).sum()
    }

    /// Whether the plan is empty (no resources, no conflicts).
    pub fn is_empty(&self) -> bool {
        self.resources.is_empty() && self.conflicts.is_empty()
    }

    /// Whether a run of this plan should be refused up front:
    /// empty plans import nothing.
    pub fn refuses_empty_run(&self) -> bool {
        self.resources.is_empty()
    }
}

/// One resource that was committed by an import run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportedResource {
    /// Run that imported the resource.
    pub run_id: MigrationRunId,
    /// Kind of the imported resource.
    pub kind: ImportedResourceKind,
    /// Natural key inside the source.
    pub source_key: String,
    /// Reference id in the target bounded context (site_id,
    /// db_id, user_id, …).
    pub ref_id: Uuid,
    /// Whether the import of this resource required a rollback
    /// undo (i.e. the resource was created then removed).
    pub rolled_back: bool,
}

impl ImportedResource {
    /// Build a committed import record.
    pub fn new(
        run_id: MigrationRunId,
        kind: ImportedResourceKind,
        source_key: impl Into<String>,
        ref_id: Uuid,
    ) -> Self {
        Self {
            run_id,
            kind,
            source_key: source_key.into(),
            ref_id,
            rolled_back: false,
        }
    }

    /// Restore a record from persistence.
    pub fn restore(
        run_id: MigrationRunId,
        kind: ImportedResourceKind,
        source_key: String,
        ref_id: Uuid,
        rolled_back: bool,
    ) -> Self {
        Self {
            run_id,
            kind,
            source_key,
            ref_id,
            rolled_back,
        }
    }

    /// Reference id in the target bounded context.
    pub fn ref_id(&self) -> Uuid {
        self.ref_id
    }

    /// Mark the resource as rolled back (the run undo deleted it).
    pub fn mark_rolled_back(&mut self) {
        self.rolled_back = true;
    }
}

/// A redacted entry in the translation log. Diagnostics are
/// scrubbed so no credential or secret material leaks into the
/// persistent log.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranslationLogEntry {
    /// Run that produced the entry.
    pub run_id: MigrationRunId,
    /// Kind of the resource being translated.
    pub kind: ImportedResourceKind,
    /// Natural key inside the source.
    pub source_key: String,
    /// Outcome of the per-resource translation.
    pub outcome: TranslationOutcome,
    /// Redacted diagnostic message.
    pub redacted: String,
}

/// Per-resource translation outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TranslationOutcome {
    /// The resource was imported.
    Imported,
    /// The resource was skipped (conflict, missing dependency).
    Skipped,
    /// The resource import failed and the run rolled back.
    Failed,
}

/// The translation log for one import run. Append-only within the
/// run; the whole log is persisted with redacted diagnostics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranslationLog {
    run_id: MigrationRunId,
    entries: Vec<TranslationLogEntry>,
}

impl TranslationLog {
    /// Begin a log for a run.
    pub fn new(run_id: MigrationRunId) -> Self {
        Self {
            run_id,
            entries: Vec::new(),
        }
    }

    /// Run identifier.
    pub fn run_id(&self) -> MigrationRunId {
        self.run_id
    }

    /// Entries in insertion order.
    pub fn entries(&self) -> &[TranslationLogEntry] {
        &self.entries
    }

    /// Append a redacted entry, stamping the run id.
    pub fn push(&mut self, entry: TranslationLogEntry) {
        self.entries.push(TranslationLogEntry {
            run_id: self.run_id,
            ..entry
        });
    }

    /// Whether any entry reported a failure. When true the run
    /// must roll back the per-resource commit point.
    pub fn has_failure(&self) -> bool {
        self.entries
            .iter()
            .any(|e| matches!(e.outcome, TranslationOutcome::Failed))
    }
}

/// Contract implemented by each importer driver. The driver parses
/// the source bundle, produces a preview plan, and commits each
/// planned resource through the per-bounded-context translators.
#[async_trait]
pub trait MigrationDriver: Send + Sync + 'static {
    /// Raw bytes the driver inspects for sniffing.
    type Source: Send + Sync;

    /// Detect whether this driver recognizes `source`. Used both
    /// for auto-detection and for the `driver_hint` confirmation.
    fn sniff(&self, source: &Self::Source) -> Option<DriverKind>;

    /// Parse the source and produce a preview plan. This is a
    /// read-only pass; nothing is committed.
    async fn dry_run(&self, source: &Self::Source) -> Result<MigrationPlan, MigrationError>;

    /// Commit the confirmed plan. The implementation MUST write
    /// every resource within a single transaction and return the
    /// imported resource set; on failure the transaction is rolled
    /// back to the per-resource commit point and the error reports
    /// the failing source key.
    async fn run(
        &self,
        source: &Self::Source,
        plan: &MigrationPlan,
        run_id: MigrationRunId,
        target_owner_user_id: Uuid,
        confirmed_at: DateTime<Utc>,
    ) -> Result<Vec<ImportedResource>, MigrationError>;

    /// Undo a committed run, deleting each imported resource by
    /// reference id in reverse insertion order.
    async fn rollback(
        &self,
        imported: &[ImportedResource],
        run_id: MigrationRunId,
        confirmed_at: DateTime<Utc>,
    ) -> Result<Vec<ImportedResource>, MigrationError>;
}

/// Persistence port for import runs and their translation logs.
#[async_trait]
pub trait MigrationRepository: Send + Sync + 'static {
    /// Insert an import run header (driver, plan, target owner,
    /// timestamps, status).
    async fn insert_run(&self, run: &MigrationRun) -> Result<(), MigrationError>;
    /// Replace the status of an existing run header.
    async fn update_run_status(&self, run: &MigrationRun) -> Result<(), MigrationError>;
    /// Append a translation log entry.
    async fn insert_log_entry(&self, entry: &TranslationLogEntry) -> Result<(), MigrationError>;
    /// Insert a committed imported resource.
    async fn insert_imported_resource(
        &self,
        resource: &ImportedResource,
    ) -> Result<(), MigrationError>;
    /// Mark an imported resource as rolled back.
    async fn mark_imported_resource_rolled_back(
        &self,
        resource: &ImportedResource,
    ) -> Result<(), MigrationError>;
    /// List recent runs within the retention window (newest first).
    async fn recent_runs(&self, limit: i64) -> Result<Vec<MigrationRun>, MigrationError>;
    /// Load the imported resources for a run (for rollback).
    async fn imported_resources(
        &self,
        run_id: MigrationRunId,
    ) -> Result<Vec<ImportedResource>, MigrationError>;
}

/// A persisted import run header.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MigrationRun {
    run_id: MigrationRunId,
    plan_id: MigrationPlanId,
    driver: DriverKind,
    target_owner_user_id: Uuid,
    confirmed_at: DateTime<Utc>,
    status: MigrationRunStatus,
}

impl MigrationRun {
    /// Build a run header.
    pub fn new(
        run_id: MigrationRunId,
        plan_id: MigrationPlanId,
        driver: DriverKind,
        target_owner_user_id: Uuid,
        confirmed_at: DateTime<Utc>,
    ) -> Self {
        Self {
            run_id,
            plan_id,
            driver,
            target_owner_user_id,
            confirmed_at,
            status: MigrationRunStatus::InProgress,
        }
    }

    /// Run identifier.
    pub fn run_id(&self) -> MigrationRunId {
        self.run_id
    }

    /// Plan identifier that produced this run.
    pub fn plan_id(&self) -> MigrationPlanId {
        self.plan_id
    }

    /// Driver that performed the run.
    pub fn driver(&self) -> DriverKind {
        self.driver
    }

    /// Target owner for every imported resource.
    pub fn target_owner_user_id(&self) -> Uuid {
        self.target_owner_user_id
    }

    /// When the operator confirmed the run.
    pub fn confirmed_at(&self) -> DateTime<Utc> {
        self.confirmed_at
    }

    /// Lifecycle status.
    pub fn status(&self) -> MigrationRunStatus {
        self.status
    }

    /// Serialized status for wire output.
    pub fn status_str(&self) -> &'static str {
        match self.status {
            MigrationRunStatus::InProgress => "in_progress",
            MigrationRunStatus::Completed => "completed",
            MigrationRunStatus::Failed => "failed",
            MigrationRunStatus::RolledBack => "rolled_back",
        }
    }

    /// Mark the run as completed.
    pub fn mark_completed(&mut self) {
        self.status = MigrationRunStatus::Completed;
    }

    /// Mark the run as failed and rolled back.
    pub fn mark_failed(&mut self) {
        self.status = MigrationRunStatus::Failed;
    }

    /// Mark the run as rolled back.
    pub fn mark_rolled_back(&mut self) {
        self.status = MigrationRunStatus::RolledBack;
    }

    /// Restore a run header from persistence.
    #[allow(clippy::too_many_arguments)]
    pub fn restore(
        run_id: MigrationRunId,
        plan_id: MigrationPlanId,
        driver: DriverKind,
        target_owner_user_id: Uuid,
        confirmed_at: DateTime<Utc>,
        status: MigrationRunStatus,
    ) -> Self {
        Self {
            run_id,
            plan_id,
            driver,
            target_owner_user_id,
            confirmed_at,
            status,
        }
    }
}

/// Lifecycle status of an import run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MigrationRunStatus {
    /// The run is executing.
    InProgress,
    /// The run committed successfully.
    Completed,
    /// The run failed and was rolled back.
    Failed,
    /// A completed run was rolled back by the operator.
    RolledBack,
}

/// Errors that can occur in the migration importers bounded context.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum MigrationError {
    /// The source key is invalid (empty or too long).
    #[error("invalid source key: {0}")]
    InvalidSourceKey(String),
    /// The driver was asked to sniff/parse a source it does not
    /// recognize.
    #[error("source does not match driver {0}")]
    UnknownSource(&'static str),
    /// The source bundle is malformed.
    #[error("malformed source bundle: {0}")]
    MalformedSource(String),
    /// A confirmed plan expired before the run started (60s TTL).
    #[error("plan {0} expired; preview again")]
    PlanExpired(MigrationPlanId),
    /// The same source+plan pair was already imported.
    #[error("source already imported by run {0}")]
    AlreadyImported(MigrationRunId),
    /// A target resource already exists (conflict).
    #[error("target resource {0} already exists")]
    TargetConflict(String),
    /// Rollback is only allowed within the retention window.
    #[error("rollback window expired for run {0}")]
    RollbackWindowExpired(MigrationRunId),
    /// Persistence layer failure.
    #[error("migration persistence error: {0}")]
    Persistence(String),
}

impl From<RepoError> for MigrationError {
    fn from(error: RepoError) -> Self {
        MigrationError::Persistence(error.0)
    }
}

/// Convenience alias so callers do not carry the full trait bound.
pub type SharedMigrationDriver = Box<dyn MigrationDriver<Source = Box<dyn MigrationSource>>>;

/// Marker for a driver source. The concrete drivers own the
/// tar/gzip representation; this type only pins the object-safe
/// bound.
pub trait MigrationSource: Send + Sync {}
impl<T: Send + Sync> MigrationSource for T {}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    fn sample_plan() -> MigrationPlan {
        MigrationPlan::new(
            MigrationPlanId::new(),
            DriverKind::CpanelPkgacct,
            vec![
                PlannedResource::new(ImportedResourceKind::Site, "example.com", "vhost", 4096)
                    .unwrap(),
            ],
            Vec::new(),
            Vec::new(),
            Utc::now(),
        )
    }

    #[test]
    fn planned_resource_rejects_empty_key() {
        let err =
            PlannedResource::new(ImportedResourceKind::User, "", "u", 0).expect_err("must reject");
        assert!(matches!(err, MigrationError::InvalidSourceKey(_)));
    }

    #[test]
    fn plan_reports_total_bytes() {
        let plan = sample_plan();
        assert_eq!(plan.total_bytes(), 4096);
    }

    #[test]
    fn plan_refuses_empty_run() {
        let empty = MigrationPlan::new(
            MigrationPlanId::new(),
            DriverKind::BaotaBackup,
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Utc::now(),
        );
        assert!(empty.refuses_empty_run());
        assert!(!sample_plan().refuses_empty_run());
    }

    #[test]
    fn translation_log_reports_failure() {
        let run_id = MigrationRunId::new();
        let mut log = TranslationLog::new(run_id);
        log.push(TranslationLogEntry {
            run_id,
            kind: ImportedResourceKind::Site,
            source_key: "a.com".to_string(),
            outcome: TranslationOutcome::Imported,
            redacted: "ok".to_string(),
        });
        assert!(!log.has_failure());
        log.push(TranslationLogEntry {
            run_id,
            kind: ImportedResourceKind::Database,
            source_key: "db1".to_string(),
            outcome: TranslationOutcome::Failed,
            redacted: "refused".to_string(),
        });
        assert!(log.has_failure());
        assert!(log.entries().iter().all(|e| e.run_id == run_id));
    }

    #[test]
    fn imported_resource_mark_rolled_back() {
        let mut resource = ImportedResource::new(
            MigrationRunId::new(),
            ImportedResourceKind::Site,
            "example.com",
            Uuid::new_v4(),
        );
        assert!(!resource.rolled_back);
        resource.mark_rolled_back();
        assert!(resource.rolled_back);
    }

    #[test]
    fn run_status_lifecycle() {
        let mut run = MigrationRun::new(
            MigrationRunId::new(),
            MigrationPlanId::new(),
            DriverKind::CpanelLegacyBackup,
            Uuid::new_v4(),
            Utc::now(),
        );
        assert!(matches!(run.status(), MigrationRunStatus::InProgress));
        run.mark_completed();
        assert!(matches!(run.status(), MigrationRunStatus::Completed));
        run.mark_rolled_back();
        assert!(matches!(run.status(), MigrationRunStatus::RolledBack));
    }

    #[test]
    fn driver_kind_wire_names() {
        assert_eq!(DriverKind::CpanelPkgacct.as_str(), "cpanel-pkgacct");
        assert_eq!(
            DriverKind::CpanelLegacyBackup.as_str(),
            "cpanel-legacy-backup"
        );
        assert_eq!(DriverKind::BaotaBackup.as_str(), "baota-backup");
        assert_eq!(
            DriverKind::TarWithJsonManifest.as_str(),
            "tar-with-json-manifest"
        );
    }

    #[test]
    fn btree_map_keyed_by_resource_kind() {
        let mut counts: BTreeMap<ImportedResourceKind, u32> = BTreeMap::new();
        *counts.entry(ImportedResourceKind::Site).or_insert(0) += 1;
        *counts.entry(ImportedResourceKind::Site).or_insert(0) += 1;
        *counts.entry(ImportedResourceKind::Database).or_insert(0) += 1;
        assert_eq!(counts.get(&ImportedResourceKind::Site), Some(&2));
        assert_eq!(counts.len(), 2);
    }
}
