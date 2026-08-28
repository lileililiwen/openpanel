//! Backup plans, atomic local capture, verification, retention, and restore preflight.

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use chrono::Utc;
use openpanel_core::{AuditAction, AuditEvent, AuditOutcome, AuditService};
use openpanel_domain::backups::{
    BackupArtifact, BackupManifest, BackupPlan, BackupResource, BackupRun, BackupRunState,
    RestorePath, RetentionCandidate, RetentionPolicy,
};
use serde::{Deserialize, Serialize};
use sqlx::{Pool, Row, Sqlite};
use thiserror::Error;
use uuid::Uuid;

use crate::cron::{CronInput, CronService};

/// Plan creation fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupPlanInput {
    /// Display name.
    pub name: String,
    /// Selected resources.
    pub resources: Vec<BackupResource>,
    /// Five-field cron expression.
    pub schedule: String,
    /// IANA timezone.
    pub timezone: String,
    /// Number of completed copies to retain.
    pub retention_copies: usize,
}

/// Mutable plan fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupPlanUpdate {
    /// Optional name.
    pub name: Option<String>,
    /// Optional retention count.
    pub retention_copies: Option<usize>,
}

/// Restore preflight result.
#[derive(Debug, Clone, Serialize)]
pub struct RestorePreview {
    /// Whether checksums and version pass.
    pub ready: bool,
    /// Conflicting target descriptions.
    pub conflicts: Vec<String>,
    /// Estimated artifact bytes.
    pub required_bytes: u64,
}

/// Restore resource selection and conflict behavior.
#[derive(Debug, Clone, Deserialize)]
pub struct RestoreInput {
    /// Empty selects every artifact.
    #[serde(default)]
    pub resources: Vec<BackupResource>,
    /// `fail` or `overwrite`.
    pub conflict_policy: String,
    /// Required only for Owner overwrite.
    pub confirmation_token: Option<String>,
}

/// Restore job metadata.
#[derive(Debug, Clone, Serialize)]
pub struct RestoreJob {
    /// Restore id.
    pub id: Uuid,
    /// Source run id.
    pub run_id: Uuid,
    /// Current state.
    pub state: String,
}

/// Typed service failures.
#[derive(Debug, Error)]
pub enum BackupServiceError {
    /// Invalid input or unsafe state.
    #[error("backup validation failed: {0}")]
    Validation(String),
    /// Hidden or absent record.
    #[error("backup record not found")]
    NotFound,
    /// Artifact is corrupt.
    #[error("backup artifact is corrupt")]
    Corrupt,
    /// Storage or filesystem failure.
    #[error("backup operation failed: {0}")]
    Internal(String),
}

/// Backup use-case service over SQLite and a local destination.
#[derive(Clone)]
pub struct BackupService {
    pool: Pool<Sqlite>,
    root: PathBuf,
    audit: Arc<dyn AuditService>,
    master_key: Option<[u8; 32]>,
    cron: Option<(Arc<CronService>, PathBuf)>,
}
impl BackupService {
    /// Construct a local backup service.
    pub fn new(
        pool: Pool<Sqlite>,
        root: PathBuf,
        audit: Arc<dyn AuditService>,
        master_key: Option<[u8; 32]>,
        cron: Option<(Arc<CronService>, PathBuf)>,
    ) -> Self {
        Self {
            pool,
            root,
            audit,
            master_key,
            cron,
        }
    }

    /// Backup storage root.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Create a plan.
    pub async fn create_plan(
        &self,
        owner: Uuid,
        input: BackupPlanInput,
    ) -> Result<BackupPlan, BackupServiceError> {
        let mut plan = BackupPlan::new(
            Uuid::new_v4(),
            owner,
            input.name,
            input.resources,
            input.schedule,
            input.timezone,
            RetentionPolicy::copies(input.retention_copies).map_err(validation)?,
            Utc::now(),
        )
        .map_err(validation)?;
        if let Some((cron, working_root)) = &self.cron {
            let executable = std::env::current_exe().map_err(io)?;
            let job = cron
                .create(
                    owner,
                    CronInput {
                        name: format!("Backup: {}", plan.name()),
                        schedule: plan.schedule().into(),
                        timezone: plan.timezone().into(),
                        kind: "command".into(),
                        executable: Some(executable.to_string_lossy().into_owned()),
                        arguments: vec![
                            "backup".into(),
                            "run".into(),
                            "--plan-id".into(),
                            plan.id().to_string(),
                        ],
                        working_directory: Some(working_root.to_string_lossy().into_owned()),
                        url: None,
                        method: None,
                        timeout_secs: 86_400,
                        overlap_policy: "skip".into(),
                    },
                )
                .await
                .map_err(|error| BackupServiceError::Internal(error.to_string()))?;
            plan.link_cron(job.id(), Utc::now());
        }
        sqlx::query("INSERT INTO backup_plans (id, owner_id, payload) VALUES (?, ?, ?)")
            .bind(plan.id().to_string())
            .bind(owner.to_string())
            .bind(plan.to_json().map_err(validation)?)
            .execute(&self.pool)
            .await
            .map_err(db)?;
        self.audit(owner, AuditAction::BackupChanged, plan.id(), "created")
            .await;
        Ok(plan)
    }

    /// List visible plans.
    pub async fn plans(
        &self,
        owner: Uuid,
        all: bool,
    ) -> Result<Vec<BackupPlan>, BackupServiceError> {
        let rows = if all {
            sqlx::query("SELECT payload FROM backup_plans ORDER BY id")
                .fetch_all(&self.pool)
                .await
        } else {
            sqlx::query("SELECT payload FROM backup_plans WHERE owner_id = ? ORDER BY id")
                .bind(owner.to_string())
                .fetch_all(&self.pool)
                .await
        }
        .map_err(db)?;
        rows.into_iter()
            .map(|row| BackupPlan::from_json(row.get::<String, _>(0).as_str()).map_err(validation))
            .collect()
    }

    /// Get one visible plan.
    pub async fn plan(
        &self,
        owner: Uuid,
        all: bool,
        id: Uuid,
    ) -> Result<BackupPlan, BackupServiceError> {
        let row = if all {
            sqlx::query("SELECT payload FROM backup_plans WHERE id = ?")
                .bind(id.to_string())
                .fetch_optional(&self.pool)
                .await
        } else {
            sqlx::query("SELECT payload FROM backup_plans WHERE id = ? AND owner_id = ?")
                .bind(id.to_string())
                .bind(owner.to_string())
                .fetch_optional(&self.pool)
                .await
        }
        .map_err(db)?
        .ok_or(BackupServiceError::NotFound)?;
        BackupPlan::from_json(row.get::<String, _>(0).as_str()).map_err(validation)
    }

    /// Update a plan.
    pub async fn update_plan(
        &self,
        owner: Uuid,
        all: bool,
        id: Uuid,
        update: BackupPlanUpdate,
    ) -> Result<BackupPlan, BackupServiceError> {
        let mut plan = self.plan(owner, all, id).await?;
        let retention = update
            .retention_copies
            .map(RetentionPolicy::copies)
            .transpose()
            .map_err(validation)?;
        plan.update(update.name, retention, Utc::now())
            .map_err(validation)?;
        self.save_plan(&plan).await?;
        self.audit(owner, AuditAction::BackupChanged, id, "updated")
            .await;
        Ok(plan)
    }

    /// Enable or disable a plan.
    pub async fn set_enabled(
        &self,
        owner: Uuid,
        all: bool,
        id: Uuid,
        enabled: bool,
    ) -> Result<BackupPlan, BackupServiceError> {
        let mut plan = self.plan(owner, all, id).await?;
        if enabled {
            plan.enable();
        } else {
            plan.disable();
        }
        if let Some(cron_id) = plan.cron_job_id()
            && let Some((cron, _)) = &self.cron
        {
            cron.set_enabled(owner, all, cron_id, enabled)
                .await
                .map_err(|error| BackupServiceError::Internal(error.to_string()))?;
        }
        self.save_plan(&plan).await?;
        self.audit(
            owner,
            AuditAction::BackupChanged,
            id,
            if enabled { "enabled" } else { "disabled" },
        )
        .await;
        Ok(plan)
    }

    async fn save_plan(&self, plan: &BackupPlan) -> Result<(), BackupServiceError> {
        sqlx::query("UPDATE backup_plans SET payload = ? WHERE id = ?")
            .bind(plan.to_json().map_err(validation)?)
            .bind(plan.id().to_string())
            .execute(&self.pool)
            .await
            .map_err(db)?;
        Ok(())
    }

    /// Delete a plan with no active run.
    pub async fn delete_plan(
        &self,
        owner: Uuid,
        all: bool,
        id: Uuid,
    ) -> Result<(), BackupServiceError> {
        let plan = self.plan(owner, all, id).await?;
        if let Some(cron_id) = plan.cron_job_id()
            && let Some((cron, _)) = &self.cron
        {
            cron.delete(owner, all, cron_id)
                .await
                .map_err(|error| BackupServiceError::Internal(error.to_string()))?;
        }
        sqlx::query("DELETE FROM backup_plans WHERE id = ?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(db)?;
        self.audit(owner, AuditAction::BackupChanged, id, "deleted")
            .await;
        Ok(())
    }

    /// Capture one plan into isolated staging and atomically finalize it.
    pub async fn run_plan(
        &self,
        owner: Uuid,
        all: bool,
        plan_id: Uuid,
    ) -> Result<BackupRun, BackupServiceError> {
        let plan = self.plan(owner, all, plan_id).await?;
        let now = Utc::now();
        let mut run = BackupRun::new(Uuid::new_v4(), plan_id, plan.owner_id(), now);
        run.start(now).map_err(validation)?;
        self.save_new_run(&run, now).await?;
        let staging = self.root.join(".staging").join(run.id().to_string());
        let final_dir = self.root.join("runs").join(run.id().to_string());
        let result = self.capture(&plan, &mut run, &staging, &final_dir).await;
        if let Err(error) = result {
            let _ = tokio::fs::remove_dir_all(&staging).await;
            run.fail(Utc::now(), "resource capture failed")
                .map_err(validation)?;
            self.save_run(&run).await?;
            return Err(error);
        }
        self.save_run(&run).await?;
        self.audit(owner, AuditAction::BackupRun, run.id(), "completed")
            .await;
        self.apply_retention(&plan).await?;
        Ok(run)
    }

    async fn capture(
        &self,
        plan: &BackupPlan,
        run: &mut BackupRun,
        staging: &Path,
        final_dir: &Path,
    ) -> Result<(), BackupServiceError> {
        tokio::fs::create_dir_all(staging).await.map_err(io)?;
        for resource in plan.resources() {
            let artifact = match resource {
                BackupResource::PanelMetadata => {
                    let name = "panel.json".to_string();
                    let bytes = br#"{"format_version":1,"secrets":"redacted"}"#.to_vec();
                    let path = staging.join(&name);
                    let mut file = tokio::fs::File::create(&path).await.map_err(io)?;
                    use tokio::io::AsyncWriteExt;
                    file.write_all(&bytes).await.map_err(io)?;
                    file.sync_all().await.map_err(io)?;
                    BackupArtifact::new(resource.clone(), name, &bytes).map_err(validation)?
                }
                BackupResource::Site(id) => {
                    let row = sqlx::query("SELECT owner_id, document_root FROM sites WHERE id = ?")
                        .bind(id.to_string())
                        .fetch_optional(&self.pool)
                        .await
                        .map_err(db)?
                        .ok_or(BackupServiceError::NotFound)?;
                    if row.get::<String, _>(0) != plan.owner_id().to_string() {
                        return Err(BackupServiceError::NotFound);
                    }
                    let source = PathBuf::from(row.get::<String, _>(1));
                    let name = format!("site-{id}.opfs");
                    let target = staging.join(&name);
                    let selected = resource.clone();
                    tokio::task::spawn_blocking(move || {
                        stream_site_archive(&source, &target, selected, name)
                    })
                    .await
                    .map_err(|error| BackupServiceError::Internal(error.to_string()))??
                }
                BackupResource::Database(id) => {
                    self.capture_database(plan.owner_id(), *id, staging).await?
                }
            };
            run.add_artifact(artifact).map_err(validation)?;
        }
        let manifest = BackupManifest::new(run.id(), run.artifacts().to_vec(), Utc::now())
            .map_err(validation)?;
        tokio::fs::write(
            staging.join("manifest.json"),
            manifest.to_json().map_err(validation)?,
        )
        .await
        .map_err(io)?;
        run.mark_finalized().map_err(validation)?;
        if let Some(parent) = final_dir.parent() {
            tokio::fs::create_dir_all(parent).await.map_err(io)?;
        }
        tokio::fs::rename(staging, final_dir).await.map_err(io)?;
        run.complete(Utc::now()).map_err(validation)?;
        Ok(())
    }

    async fn capture_database(
        &self,
        owner: Uuid,
        id: Uuid,
        staging: &Path,
    ) -> Result<BackupArtifact, BackupServiceError> {
        let key = self.master_key.ok_or_else(|| {
            BackupServiceError::Validation(
                "database backup requires the configured master key".into(),
            )
        })?;
        let row = sqlx::query("SELECT owner_id, name, db_user, db_host, password_ciphertext FROM databases WHERE id = ?").bind(id.to_string()).fetch_optional(&self.pool).await.map_err(db)?.ok_or(BackupServiceError::NotFound)?;
        if row.get::<String, _>(0) != owner.to_string() {
            return Err(BackupServiceError::NotFound);
        }
        let name: String = row.get(1);
        let user: String = row.get(2);
        let host: String = row.get(3);
        let encrypted: String = row.get(4);
        let password =
            crate::databases::crypto::decrypt_from_storage(&key, &encrypted).map_err(|_| {
                BackupServiceError::Internal("database credential decryption failed".into())
            })?;
        let defaults = staging.join(format!(".mysql-{id}.cnf"));
        write_mysql_defaults(&defaults, &user, &host, &password)?;
        let artifact_name = format!("database-{id}.sql");
        let target = staging.join(&artifact_name);
        let output = tokio::process::Command::new("mysqldump")
            .arg(format!("--defaults-extra-file={}", defaults.display()))
            .args([
                "--single-transaction",
                "--routines",
                "--events",
                "--databases",
                name.as_str(),
            ])
            .env_clear()
            .output()
            .await;
        let _ = tokio::fs::remove_file(&defaults).await;
        let output =
            output.map_err(|_| BackupServiceError::Internal("mysqldump is unavailable".into()))?;
        if !output.status.success() {
            return Err(BackupServiceError::Internal("mysqldump failed".into()));
        }
        let mut file = tokio::fs::File::create(&target).await.map_err(io)?;
        use tokio::io::AsyncWriteExt;
        file.write_all(&output.stdout).await.map_err(io)?;
        file.sync_all().await.map_err(io)?;
        BackupArtifact::new(BackupResource::Database(id), artifact_name, &output.stdout)
            .map_err(validation)
    }

    async fn save_new_run(
        &self,
        run: &BackupRun,
        now: chrono::DateTime<Utc>,
    ) -> Result<(), BackupServiceError> {
        sqlx::query("INSERT INTO backup_runs (id, plan_id, owner_id, payload, created_at) VALUES (?, ?, ?, ?, ?)").bind(run.id().to_string()).bind(run.plan_id().to_string()).bind(run.owner_id().to_string()).bind(run.to_json().map_err(validation)?).bind(now.to_rfc3339()).execute(&self.pool).await.map_err(db)?;
        Ok(())
    }

    async fn save_run(&self, run: &BackupRun) -> Result<(), BackupServiceError> {
        sqlx::query("UPDATE backup_runs SET payload = ? WHERE id = ?")
            .bind(run.to_json().map_err(validation)?)
            .bind(run.id().to_string())
            .execute(&self.pool)
            .await
            .map_err(db)?;
        Ok(())
    }

    /// List visible runs.
    pub async fn runs(&self, owner: Uuid, all: bool) -> Result<Vec<BackupRun>, BackupServiceError> {
        let rows = if all {
            sqlx::query("SELECT payload FROM backup_runs ORDER BY created_at DESC")
                .fetch_all(&self.pool)
                .await
        } else {
            sqlx::query(
                "SELECT payload FROM backup_runs WHERE owner_id = ? ORDER BY created_at DESC",
            )
            .bind(owner.to_string())
            .fetch_all(&self.pool)
            .await
        }
        .map_err(db)?;
        rows.into_iter()
            .map(|row| BackupRun::from_json(row.get::<String, _>(0).as_str()).map_err(validation))
            .collect()
    }

    /// Get a visible run.
    pub async fn run(
        &self,
        owner: Uuid,
        all: bool,
        id: Uuid,
    ) -> Result<BackupRun, BackupServiceError> {
        self.runs(owner, all)
            .await?
            .into_iter()
            .find(|run| run.id() == id)
            .ok_or(BackupServiceError::NotFound)
    }

    /// Verify all finalized artifacts and mark corruption.
    pub async fn verify(
        &self,
        owner: Uuid,
        all: bool,
        id: Uuid,
    ) -> Result<BackupRun, BackupServiceError> {
        let mut run = self.run(owner, all, id).await?;
        if run.state() != BackupRunState::Completed {
            return Err(BackupServiceError::Corrupt);
        }
        let dir = self.root.join("runs").join(id.to_string());
        for artifact in run.artifacts() {
            let bytes = tokio::fs::read(dir.join(artifact.path()))
                .await
                .map_err(io)?;
            if !artifact.verify(&bytes) {
                run.mark_corrupt(Utc::now()).map_err(validation)?;
                self.save_run(&run).await?;
                return Err(BackupServiceError::Corrupt);
            }
        }
        Ok(run)
    }

    /// Restore preflight with integrity and byte estimate.
    pub async fn restore_preview(
        &self,
        owner: Uuid,
        all: bool,
        id: Uuid,
    ) -> Result<RestorePreview, BackupServiceError> {
        let run = self.verify(owner, all, id).await?;
        let mut conflicts = Vec::new();
        for artifact in run.artifacts() {
            if let BackupResource::Site(site_id) = artifact.resource() {
                let row = sqlx::query("SELECT owner_id, document_root FROM sites WHERE id = ?")
                    .bind(site_id.to_string())
                    .fetch_optional(&self.pool)
                    .await
                    .map_err(db)?
                    .ok_or(BackupServiceError::NotFound)?;
                if row.get::<String, _>(0) != run.owner_id().to_string() {
                    return Err(BackupServiceError::NotFound);
                }
                let target = PathBuf::from(row.get::<String, _>(1));
                if target.exists() && std::fs::read_dir(&target).map_err(io)?.next().is_some() {
                    conflicts.push(format!("site:{site_id}"));
                }
            }
        }
        Ok(RestorePreview {
            ready: conflicts.is_empty(),
            conflicts,
            required_bytes: run.artifacts().iter().map(BackupArtifact::size).sum(),
        })
    }

    /// Start a safe, fail-on-conflict restore job.
    pub async fn restore(
        &self,
        owner: Uuid,
        all: bool,
        id: Uuid,
        input: RestoreInput,
    ) -> Result<RestoreJob, BackupServiceError> {
        let run = self.verify(owner, all, id).await?;
        if input.conflict_policy != "fail" && input.conflict_policy != "overwrite" {
            return Err(BackupServiceError::Validation(
                "conflict_policy must be fail or overwrite".into(),
            ));
        }
        if input.conflict_policy == "overwrite" {
            if !all {
                return Err(BackupServiceError::Validation(
                    "only an Owner may confirm overwrite".into(),
                ));
            }
            openpanel_domain::backups::OverwriteConfirmation::new(
                input.confirmation_token.unwrap_or_default(),
            )
            .map_err(validation)?;
        }
        if !input.resources.is_empty()
            && input.resources.iter().any(|selected| {
                !run.artifacts()
                    .iter()
                    .any(|artifact| artifact.resource() == selected)
            })
        {
            return Err(BackupServiceError::Validation(
                "selected resource is not in this backup".into(),
            ));
        }
        let preview = self.restore_preview(owner, all, id).await?;
        if !preview.conflicts.is_empty() && input.conflict_policy == "fail" {
            return Err(BackupServiceError::Validation(
                "restore conflicts require explicit Owner overwrite confirmation".into(),
            ));
        }
        let job_id = Uuid::new_v4();
        let selected: Vec<_> = run
            .artifacts()
            .iter()
            .filter(|artifact| {
                input.resources.is_empty() || input.resources.contains(artifact.resource())
            })
            .cloned()
            .collect();
        for artifact in selected {
            match artifact.resource() {
                BackupResource::PanelMetadata => {}
                BackupResource::Site(site_id) => {
                    self.restore_site(
                        run.owner_id(),
                        run.id(),
                        *site_id,
                        &artifact,
                        job_id,
                        input.conflict_policy == "overwrite",
                    )
                    .await?
                }
                BackupResource::Database(_) => {
                    return Err(BackupServiceError::Validation(
                        "database restore adapter is not available".into(),
                    ));
                }
            }
        }
        let job = RestoreJob {
            id: job_id,
            run_id: id,
            state: "completed".into(),
        };
        sqlx::query("INSERT INTO restore_jobs (id, run_id, owner_id, state, created_at) VALUES (?, ?, ?, ?, ?)").bind(job.id.to_string()).bind(id.to_string()).bind(owner.to_string()).bind(&job.state).bind(Utc::now().to_rfc3339()).execute(&self.pool).await.map_err(db)?;
        self.audit(owner, AuditAction::BackupRestore, id, "started")
            .await;
        Ok(job)
    }

    async fn restore_site(
        &self,
        owner: Uuid,
        run_id: Uuid,
        site_id: Uuid,
        artifact: &BackupArtifact,
        job_id: Uuid,
        overwrite: bool,
    ) -> Result<(), BackupServiceError> {
        let row = sqlx::query("SELECT owner_id, document_root FROM sites WHERE id = ?")
            .bind(site_id.to_string())
            .fetch_optional(&self.pool)
            .await
            .map_err(db)?
            .ok_or(BackupServiceError::NotFound)?;
        if row.get::<String, _>(0) != owner.to_string() {
            return Err(BackupServiceError::NotFound);
        }
        let target = PathBuf::from(row.get::<String, _>(1));
        let parent = target
            .parent()
            .ok_or_else(|| BackupServiceError::Validation("site target has no parent".into()))?
            .to_path_buf();
        let staging = parent.join(format!(".openpanel-restore-{job_id}"));
        let archive = self
            .root
            .join("runs")
            .join(run_id.to_string())
            .join(artifact.path());
        let staging_clone = staging.clone();
        tokio::task::spawn_blocking(move || extract_site_archive(&archive, &staging_clone))
            .await
            .map_err(|error| BackupServiceError::Internal(error.to_string()))??;
        let safety = parent.join(format!(".openpanel-safety-{job_id}"));
        if target.exists() {
            if overwrite {
                tokio::fs::rename(&target, &safety).await.map_err(io)?;
            } else {
                tokio::fs::remove_dir(&target).await.map_err(io)?;
            }
        }
        if let Err(error) = tokio::fs::rename(&staging, &target).await {
            if safety.exists() {
                let _ = tokio::fs::rename(&safety, &target).await;
            }
            return Err(io(error));
        }
        Ok(())
    }

    /// Delete one finalized run and its artifact directory.
    pub async fn delete_run(
        &self,
        owner: Uuid,
        all: bool,
        id: Uuid,
    ) -> Result<(), BackupServiceError> {
        let run = self.run(owner, all, id).await?;
        if matches!(
            run.state(),
            BackupRunState::Pending | BackupRunState::Running
        ) {
            return Err(BackupServiceError::Validation(
                "active run cannot be deleted".into(),
            ));
        }
        tokio::fs::remove_dir_all(self.root.join("runs").join(id.to_string()))
            .await
            .ok();
        sqlx::query("DELETE FROM backup_runs WHERE id = ?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(db)?;
        Ok(())
    }

    async fn apply_retention(&self, plan: &BackupPlan) -> Result<(), BackupServiceError> {
        let runs = self.runs(plan.owner_id(), false).await?;
        let candidates: Vec<_> = runs
            .iter()
            .filter(|run| run.plan_id() == plan.id())
            .enumerate()
            .map(|(sequence, run)| {
                if run.state() == BackupRunState::Completed {
                    RetentionCandidate::completed(run.id(), -(sequence as i64), run.pinned())
                } else {
                    RetentionCandidate::active(run.id(), -(sequence as i64))
                }
            })
            .collect();
        for id in plan.retention().deletions(&candidates) {
            self.delete_run(plan.owner_id(), false, id).await?;
        }
        Ok(())
    }

    /// Remove abandoned staging directories without following external paths.
    pub async fn cleanup_staging(&self) -> Result<(), BackupServiceError> {
        let staging = self.root.join(".staging");
        if staging.starts_with(&self.root) {
            tokio::fs::remove_dir_all(staging).await.ok();
        }
        Ok(())
    }

    async fn audit(&self, actor: Uuid, action: AuditAction, target: Uuid, operation: &str) {
        let _ = self
            .audit
            .record(
                AuditEvent::new(actor.to_string(), action, AuditOutcome::Success)
                    .target(target.to_string())
                    .metadata(serde_json::json!({"operation": operation})),
            )
            .await;
    }
}
fn validation(error: impl std::fmt::Display) -> BackupServiceError {
    BackupServiceError::Validation(error.to_string())
}
fn db(error: sqlx::Error) -> BackupServiceError {
    BackupServiceError::Internal(error.to_string())
}
fn io(error: std::io::Error) -> BackupServiceError {
    BackupServiceError::Internal(error.to_string())
}

fn stream_site_archive(
    source: &Path,
    target: &Path,
    resource: BackupResource,
    name: String,
) -> Result<BackupArtifact, BackupServiceError> {
    use std::io::{Read, Write};

    use sha2::{Digest, Sha256};
    if !source.is_dir() {
        return Err(BackupServiceError::Validation(
            "site document root is unavailable".into(),
        ));
    }
    let file = std::fs::File::create(target).map_err(io)?;
    let mut writer = std::io::BufWriter::new(file);
    writer.write_all(b"OPFS1\n").map_err(io)?;
    for entry in walkdir::WalkDir::new(source)
        .follow_links(false)
        .sort_by_file_name()
    {
        let entry = entry.map_err(|error| BackupServiceError::Internal(error.to_string()))?;
        if !entry.file_type().is_file() {
            continue;
        }
        let relative = entry
            .path()
            .strip_prefix(source)
            .map_err(|error| BackupServiceError::Internal(error.to_string()))?;
        let safe = RestorePath::new(relative).map_err(validation)?;
        let path = safe.as_str().as_bytes();
        let size = entry
            .metadata()
            .map_err(|error| BackupServiceError::Internal(error.to_string()))?
            .len();
        writer
            .write_all(&(path.len() as u32).to_be_bytes())
            .map_err(io)?;
        writer.write_all(&size.to_be_bytes()).map_err(io)?;
        writer.write_all(path).map_err(io)?;
        let mut input = std::io::BufReader::new(std::fs::File::open(entry.path()).map_err(io)?);
        std::io::copy(&mut input, &mut writer).map_err(io)?;
    }
    writer.flush().map_err(io)?;
    writer.get_ref().sync_all().map_err(io)?;
    drop(writer);
    let mut input = std::io::BufReader::new(std::fs::File::open(target).map_err(io)?);
    let mut digest = Sha256::new();
    let mut buffer = [0u8; 8192];
    let mut size = 0u64;
    loop {
        let read = input.read(&mut buffer).map_err(io)?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
        size += read as u64;
    }
    BackupArtifact::from_digest(resource, name, size, hex::encode(digest.finalize()))
        .map_err(validation)
}

fn write_mysql_defaults(
    path: &Path,
    user: &str,
    host: &str,
    password: &str,
) -> Result<(), BackupServiceError> {
    use std::{io::Write, os::unix::fs::OpenOptionsExt};
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .map_err(io)?;
    writeln!(
        file,
        "[client]\nuser={user}\nhost={host}\npassword={password}"
    )
    .map_err(io)?;
    file.sync_all().map_err(io)
}

fn extract_site_archive(archive: &Path, staging: &Path) -> Result<(), BackupServiceError> {
    use std::io::{Read, Write};
    let mut input = std::io::BufReader::new(std::fs::File::open(archive).map_err(io)?);
    let mut magic = [0u8; 6];
    input.read_exact(&mut magic).map_err(io)?;
    if &magic != b"OPFS1\n" {
        return Err(BackupServiceError::Corrupt);
    }
    std::fs::create_dir_all(staging).map_err(io)?;
    loop {
        let mut path_len = [0u8; 4];
        match input.read_exact(&mut path_len) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(error) => return Err(io(error)),
        }
        let path_len = u32::from_be_bytes(path_len) as usize;
        if path_len == 0 || path_len > 16 * 1024 {
            return Err(BackupServiceError::Corrupt);
        }
        let mut size = [0u8; 8];
        input.read_exact(&mut size).map_err(io)?;
        let size = u64::from_be_bytes(size);
        if size > 1_099_511_627_776 {
            return Err(BackupServiceError::Corrupt);
        }
        let mut path = vec![0u8; path_len];
        input.read_exact(&mut path).map_err(io)?;
        let path = std::str::from_utf8(&path).map_err(|_| BackupServiceError::Corrupt)?;
        let safe = RestorePath::new(path).map_err(|_| BackupServiceError::Corrupt)?;
        let target = staging.join(safe.as_str());
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(io)?;
        }
        let mut output = std::fs::File::create(target).map_err(io)?;
        let copied = std::io::copy(&mut input.by_ref().take(size), &mut output).map_err(io)?;
        if copied != size {
            return Err(BackupServiceError::Corrupt);
        }
        output.flush().map_err(io)?;
        output.sync_all().map_err(io)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use openpanel_test_support::MockAudit;
    use sqlx::sqlite::SqlitePoolOptions;

    use super::*;
    #[tokio::test]
    async fn panel_backup_finalizes_verifies_and_restore_previews_without_secrets() {
        let pool = SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::query(crate::migrations::BACKUPS_V001)
            .execute(&pool)
            .await
            .unwrap();
        let root = tempfile::tempdir().unwrap();
        let service = BackupService::new(
            pool,
            root.path().into(),
            Arc::new(MockAudit::stub()),
            None,
            None,
        );
        let owner = Uuid::new_v4();
        let plan = service
            .create_plan(
                owner,
                BackupPlanInput {
                    name: "nightly".into(),
                    resources: vec![BackupResource::PanelMetadata],
                    schedule: "0 2 * * *".into(),
                    timezone: "UTC".into(),
                    retention_copies: 3,
                },
            )
            .await
            .unwrap();
        let run = service.run_plan(owner, false, plan.id()).await.unwrap();
        assert_eq!(run.state(), BackupRunState::Completed);
        assert!(service.verify(owner, false, run.id()).await.is_ok());
        assert!(
            service
                .restore_preview(owner, false, run.id())
                .await
                .unwrap()
                .ready
        );
        let manifest = std::fs::read_to_string(
            root.path()
                .join("runs")
                .join(run.id().to_string())
                .join("manifest.json"),
        )
        .unwrap();
        assert!(!manifest.contains("password"));
    }

    #[tokio::test]
    async fn site_capture_streams_safe_paths_and_tampering_blocks_restore() {
        let pool = SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::query(crate::migrations::BACKUPS_V001)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("CREATE TABLE sites (id TEXT PRIMARY KEY, owner_id TEXT NOT NULL, document_root TEXT NOT NULL)").execute(&pool).await.unwrap();
        let root = tempfile::tempdir().unwrap();
        let site = root.path().join("site");
        std::fs::create_dir_all(site.join("public")).unwrap();
        std::fs::write(site.join("public/index.html"), b"safe").unwrap();
        let service = BackupService::new(
            pool.clone(),
            root.path().join("backups"),
            Arc::new(MockAudit::stub()),
            None,
            None,
        );
        let owner = Uuid::new_v4();
        let site_id = Uuid::new_v4();
        sqlx::query("INSERT INTO sites (id, owner_id, document_root) VALUES (?, ?, ?)")
            .bind(site_id.to_string())
            .bind(owner.to_string())
            .bind(site.to_string_lossy().to_string())
            .execute(&pool)
            .await
            .unwrap();
        let plan = service
            .create_plan(
                owner,
                BackupPlanInput {
                    name: "site".into(),
                    resources: vec![BackupResource::Site(site_id)],
                    schedule: "0 2 * * *".into(),
                    timezone: "UTC".into(),
                    retention_copies: 2,
                },
            )
            .await
            .unwrap();
        let run = service.run_plan(owner, false, plan.id()).await.unwrap();
        let artifact = run.artifacts().first().unwrap();
        assert!(artifact.path().ends_with(".opfs"));
        std::fs::remove_dir_all(&site).unwrap();
        service
            .restore(
                owner,
                false,
                run.id(),
                RestoreInput {
                    resources: vec![BackupResource::Site(site_id)],
                    conflict_policy: "fail".into(),
                    confirmation_token: None,
                },
            )
            .await
            .unwrap();
        assert_eq!(
            std::fs::read(site.join("public/index.html")).unwrap(),
            b"safe"
        );
        let path = root
            .path()
            .join("backups/runs")
            .join(run.id().to_string())
            .join(artifact.path());
        std::fs::write(path, b"tampered").unwrap();
        assert!(matches!(
            service.verify(owner, false, run.id()).await,
            Err(BackupServiceError::Corrupt)
        ));
        assert!(matches!(
            service.restore_preview(owner, false, run.id()).await,
            Err(BackupServiceError::Corrupt)
        ));
    }
}
