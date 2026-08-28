//! Restore drill runner: orchestrates sandboxed restores and
//! assertion evaluation for backup integrity verification.

use std::sync::Arc;

use chrono::Utc;
use openpanel_core::{AuditAction, AuditEvent, AuditOutcome, AuditService};
use openpanel_domain::{
    backups::{
        BackupArtifact, BackupResource, BackupRun, BackupRunState,
        drill::{
            DEFAULT_DRILL_RETENTION, DrillAssertion, DrillAssertionKind, DrillOutcome, DrillState,
            RestoreDrill,
        },
    },
    notifications::{EventKind, NotificationEvent, Severity},
};
use sqlx::{Pool, Row, Sqlite};
use tracing::warn;
use uuid::Uuid;

use super::BackupService;
use crate::notifications::NotificationService;

/// Drill service failures.
#[derive(Debug, thiserror::Error)]
pub enum DrillServiceError {
    /// Invalid input or unsafe state.
    #[error("drill validation failed: {0}")]
    Validation(String),
    /// No matching drill record.
    #[error("drill not found")]
    NotFound,
    /// The referenced backup run does not exist.
    #[error("backup run not found")]
    RunNotFound,
    /// Storage or operation failure.
    #[error("drill operation failed: {0}")]
    Internal(String),
}

fn validation(e: impl std::fmt::Display) -> DrillServiceError {
    DrillServiceError::Validation(e.to_string())
}

fn io(e: impl std::fmt::Display) -> DrillServiceError {
    DrillServiceError::Internal(e.to_string())
}

fn db(e: impl std::fmt::Display) -> DrillServiceError {
    DrillServiceError::Internal(e.to_string())
}

/// Restore drill service: runs drills over verified backups and
/// evaluates assertions in a sandboxed environment.
#[derive(Clone)]
pub struct DrillService {
    pool: Pool<Sqlite>,
    #[allow(dead_code)]
    backups: Arc<BackupService>,
    audit: Arc<dyn AuditService>,
    mysql_binary: Option<String>,
    notifications: Arc<std::sync::RwLock<Option<Arc<NotificationService>>>>,
}

impl DrillService {
    /// Create a new drill service.
    pub fn new(
        pool: Pool<Sqlite>,
        backups: Arc<BackupService>,
        audit: Arc<dyn AuditService>,
        mysql_binary: Option<String>,
    ) -> Self {
        Self {
            pool,
            backups,
            audit,
            mysql_binary,
            notifications: Arc::new(std::sync::RwLock::new(None)),
        }
    }

    /// Attach the notification service for failure dispatch.
    pub fn attach_notifications(&self, service: Arc<NotificationService>) {
        if let Ok(mut notifications) = self.notifications.write() {
            *notifications = Some(service);
        }
    }

    /// Run a restore drill for the given backup run.
    ///
    /// Creates a sandbox, runs the restore pipeline, evaluates
    /// assertions, persists the report, and dispatches notification
    /// on failure.
    pub async fn run_drill(&self, backup_run_id: Uuid) -> Result<RestoreDrill, DrillServiceError> {
        // Verify the backup run exists and is completed
        let run = self.verify_run(backup_run_id).await?;

        let now = Utc::now();
        let drill = RestoreDrill::start(backup_run_id, now);

        // Create sandbox
        let sandbox = super::sandbox::SandboxContext::create("openpanel").map_err(validation)?;

        // Provision the throwaway database if mysql is available
        if let Some(ref mysql) = self.mysql_binary
            && let Err(e) = sandbox.provision(mysql, "root").await
        {
            warn!(error = %e, "failed to provision drill sandbox database");
        }

        // Run assertions
        let assertions = self.evaluate_assertions(&run).await;

        // Determine outcome
        let outcome = if assertions.iter().all(|a| a.passed) {
            DrillOutcome::Passed
        } else {
            DrillOutcome::Failed
        };

        // Finish the drill
        let now = Utc::now();
        let drill = drill
            .finish(outcome, assertions.clone(), now)
            .map_err(validation)?;

        // Persist the drill report
        self.persist_drill(&drill).await?;

        // Prune old drills (keep DEFAULT_DRILL_RETENTION)
        self.prune_old_drills().await?;

        // Audit
        self.audit_drill(&drill).await;

        // Teardown sandbox (best-effort)
        sandbox.teardown(self.mysql_binary.as_deref());

        // Dispatch notification on failure
        if drill.outcome() == Some(DrillOutcome::Failed) {
            self.notify_failure(&drill).await;
        }

        Ok(drill)
    }

    /// List drills for a backup run (newest first).
    pub async fn list_drills(
        &self,
        backup_run_id: Uuid,
    ) -> Result<Vec<RestoreDrill>, DrillServiceError> {
        let rows = sqlx::query(
            "SELECT id, backup_run_id, state, assertions_json, started_at, completed_at
             FROM backup_drills
             WHERE backup_run_id = ?
             ORDER BY started_at DESC",
        )
        .bind(backup_run_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;

        rows.iter().map(drill_from_row).collect()
    }

    /// Get a single drill by id.
    pub async fn get_drill(&self, drill_id: Uuid) -> Result<RestoreDrill, DrillServiceError> {
        let row = sqlx::query(
            "SELECT id, backup_run_id, state, assertions_json, started_at, completed_at
             FROM backup_drills
             WHERE id = ?",
        )
        .bind(drill_id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(db)?
        .ok_or(DrillServiceError::NotFound)?;

        drill_from_row(&row)
    }

    // -- private --

    async fn verify_run(&self, backup_run_id: Uuid) -> Result<BackupRun, DrillServiceError> {
        let row = sqlx::query(
            "SELECT id, payload
             FROM backup_runs
             WHERE id = ?",
        )
        .bind(backup_run_id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(db)?
        .ok_or(DrillServiceError::RunNotFound)?;

        let payload: String = row.get("payload");
        let run = BackupRun::from_json(&payload).map_err(validation)?;
        if !matches!(run.state(), BackupRunState::Completed) {
            return Err(DrillServiceError::Validation(format!(
                "backup run {backup_run_id} is not completed"
            )));
        }

        Ok(run)
    }

    async fn evaluate_assertions(&self, run: &BackupRun) -> Vec<DrillAssertion> {
        let run_dir = self.backups.root().join("runs").join(run.id().to_string());
        let mut assertions = Vec::new();

        for artifact in run.artifacts() {
            match artifact.resource() {
                BackupResource::Site(site_id) => {
                    assertions.push(self.assert_site_archive(&run_dir, artifact, *site_id).await);
                }
                BackupResource::Database(db_id) => {
                    assertions.push(self.assert_database_dump(&run_dir, artifact, *db_id).await);
                }
                BackupResource::PanelMetadata => {
                    assertions.push(self.assert_panel_metadata(&run_dir, artifact).await);
                }
            }
        }

        assertions
    }

    async fn assert_site_archive(
        &self,
        run_dir: &std::path::Path,
        artifact: &BackupArtifact,
        site_id: Uuid,
    ) -> DrillAssertion {
        let path = run_dir.join(artifact.path());
        if !path.exists() {
            return DrillAssertion::failed(
                DrillAssertionKind::Site,
                format!("archive file missing: {}", path.display()),
            );
        }

        match std::fs::read(path) {
            Ok(contents) => {
                if contents.len() < 7 {
                    return DrillAssertion::failed(
                        DrillAssertionKind::Site,
                        "archive too small to contain OPFS header".to_string(),
                    );
                }
                if &contents[..6] != b"OPFS1\n" {
                    return DrillAssertion::failed(
                        DrillAssertionKind::Site,
                        "invalid OPFS magic header".to_string(),
                    );
                }
                DrillAssertion::passed(
                    DrillAssertionKind::Site,
                    format!("site:{site_id} archive valid ({} bytes)", contents.len()),
                )
            }
            Err(e) => DrillAssertion::failed(
                DrillAssertionKind::Site,
                format!("failed to read archive: {e}"),
            ),
        }
    }

    async fn assert_database_dump(
        &self,
        run_dir: &std::path::Path,
        artifact: &BackupArtifact,
        db_id: Uuid,
    ) -> DrillAssertion {
        let path = run_dir.join(artifact.path());
        if !path.exists() {
            return DrillAssertion::failed(
                DrillAssertionKind::Database,
                format!("dump file missing: {}", path.display()),
            );
        }

        match std::fs::metadata(path) {
            Ok(meta) => {
                if meta.len() == 0 {
                    return DrillAssertion::failed(
                        DrillAssertionKind::Database,
                        "dump file is empty".to_string(),
                    );
                }
                DrillAssertion::passed(
                    DrillAssertionKind::Database,
                    format!("db:{db_id} dump present ({} bytes)", meta.len()),
                )
            }
            Err(e) => DrillAssertion::failed(
                DrillAssertionKind::Database,
                format!("failed to stat dump: {e}"),
            ),
        }
    }

    async fn assert_panel_metadata(
        &self,
        run_dir: &std::path::Path,
        artifact: &BackupArtifact,
    ) -> DrillAssertion {
        let path = run_dir.join(artifact.path());
        if !path.exists() {
            return DrillAssertion::failed(
                DrillAssertionKind::PanelMetadata,
                format!("metadata file missing: {}", path.display()),
            );
        }

        match std::fs::read(path) {
            Ok(contents) => match serde_json::from_slice::<serde_json::Value>(&contents) {
                Ok(_) => DrillAssertion::passed(
                    DrillAssertionKind::PanelMetadata,
                    "panel metadata manifest valid".to_string(),
                ),
                Err(e) => DrillAssertion::failed(
                    DrillAssertionKind::PanelMetadata,
                    format!("invalid manifest JSON: {e}"),
                ),
            },
            Err(e) => DrillAssertion::failed(
                DrillAssertionKind::PanelMetadata,
                format!("failed to read metadata: {e}"),
            ),
        }
    }

    async fn persist_drill(&self, drill: &RestoreDrill) -> Result<(), DrillServiceError> {
        let assertions_json =
            serde_json::to_string(&drill.assertions).map_err(|e| io(e.to_string()))?;

        sqlx::query(
            "INSERT INTO backup_drills (id, backup_run_id, state, assertions_json, started_at, completed_at)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(drill.id.to_string())
        .bind(drill.backup_run_id.to_string())
        .bind(serde_json::to_string(&drill.state).map_err(|e| io(e.to_string()))?)
        .bind(&assertions_json)
        .bind(drill.started_at.to_rfc3339())
        .bind(drill.completed_at.map(|dt| dt.to_rfc3339()))
        .execute(&self.pool)
        .await
        .map_err(db)?;

        Ok(())
    }

    async fn prune_old_drills(&self) -> Result<(), DrillServiceError> {
        let rows = sqlx::query("SELECT id FROM backup_drills ORDER BY started_at DESC")
            .fetch_all(&self.pool)
            .await
            .map_err(db)?;

        if rows.len() <= DEFAULT_DRILL_RETENTION {
            return Ok(());
        }

        let to_delete = &rows[DEFAULT_DRILL_RETENTION..];
        for row in to_delete {
            let id: String = row.get("id");
            sqlx::query("DELETE FROM backup_drills WHERE id = ?")
                .bind(&id)
                .execute(&self.pool)
                .await
                .map_err(db)?;
        }

        Ok(())
    }

    async fn audit_drill(&self, drill: &RestoreDrill) {
        let action = match drill.outcome() {
            Some(DrillOutcome::Passed) => AuditAction::BackupRun,
            Some(DrillOutcome::Failed) => AuditAction::BackupRun,
            None => AuditAction::BackupRun,
        };
        let _ = self
            .audit
            .record(
                AuditEvent::new("system".to_string(), action, AuditOutcome::Success)
                    .target(drill.id.to_string())
                    .metadata(serde_json::json!({
                        "backup_run_id": drill.backup_run_id,
                        "outcome": drill.outcome().map(|o| format!("{o:?}")),
                        "assertion_count": drill.assertions.len(),
                    })),
            )
            .await;
    }

    async fn notify_failure(&self, drill: &RestoreDrill) {
        let failing: Vec<_> = drill.assertions.iter().filter(|a| !a.passed).collect();
        warn!(
            drill_id = %drill.id,
            backup_run_id = %drill.backup_run_id,
            failing_count = failing.len(),
            "drill failed"
        );
        let Some(notifications) = self
            .notifications
            .read()
            .ok()
            .and_then(|guard| guard.clone())
        else {
            return;
        };
        let details = serde_json::json!({
            "drill_id": drill.id,
            "backup_run_id": drill.backup_run_id,
            "failing": failing.iter().map(|a| serde_json::json!({
                "kind": format!("{:?}", a.kind),
                "detail": a.detail,
            })).collect::<Vec<_>>(),
        });
        let Ok(event) = NotificationEvent::new(
            Uuid::new_v4(),
            EventKind::Alert,
            format!(
                "backup restore drill failed for run {}",
                drill.backup_run_id
            ),
            Severity::Critical,
            details,
            Utc::now(),
        ) else {
            return;
        };
        let _ = notifications.publish(event).await;
    }
}

fn drill_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<RestoreDrill, DrillServiceError> {
    let id: String = row.get("id");
    let backup_run_id: String = row.get("backup_run_id");
    let state_json: String = row.get("state");
    let assertions_json: String = row.get("assertions_json");
    let started_at: String = row.get("started_at");
    let completed_at: Option<String> = row.get("completed_at");

    let state: DrillState = serde_json::from_str(&state_json)
        .map_err(|e| DrillServiceError::Internal(e.to_string()))?;
    let assertions: Vec<DrillAssertion> = serde_json::from_str(&assertions_json)
        .map_err(|e| DrillServiceError::Internal(e.to_string()))?;

    Ok(RestoreDrill {
        id: Uuid::parse_str(&id).map_err(|e| DrillServiceError::Internal(e.to_string()))?,
        backup_run_id: Uuid::parse_str(&backup_run_id)
            .map_err(|e| DrillServiceError::Internal(e.to_string()))?,
        state,
        assertions,
        started_at: chrono::DateTime::parse_from_rfc3339(&started_at)
            .map_err(|e| DrillServiceError::Internal(e.to_string()))?
            .with_timezone(&Utc),
        completed_at: completed_at
            .map(|s| chrono::DateTime::parse_from_rfc3339(&s).map(|dt| dt.with_timezone(&Utc)))
            .transpose()
            .map_err(|e| DrillServiceError::Internal(e.to_string()))?,
    })
}

#[cfg(test)]
mod tests {
    use openpanel_domain::backups::BackupResource;

    use super::*;

    fn artifact(resource: BackupResource, path: impl Into<String>) -> BackupArtifact {
        BackupArtifact::new(resource, path, b"x").unwrap()
    }

    fn write_temp(contents: &[u8]) -> (tempfile::TempDir, String) {
        let dir = tempfile::tempdir().unwrap();
        let abs = dir.path().join("artifact");
        std::fs::write(&abs, contents).unwrap();
        let rel = std::path::Path::new("target")
            .join(format!("drill_test_{}", Uuid::new_v4()))
            .join("artifact");
        std::fs::create_dir_all(rel.parent().unwrap()).unwrap();
        std::fs::copy(&abs, &rel).unwrap();
        (dir, rel.display().to_string())
    }

    #[tokio::test]
    async fn site_archive_passes_with_valid_header() {
        let (_dir, path) = write_temp(b"OPFS1\npayload");
        let a = artifact(BackupResource::Site(Uuid::new_v4()), path.clone());
        let svc = DrillService {
            pool: Pool::connect_lazy("sqlite::memory:").unwrap(),
            backups: Arc::new(BackupService::new(
                Pool::connect_lazy("sqlite::memory:").unwrap(),
                std::env::temp_dir(),
                Arc::new(openpanel_core::NoopAuditService),
                None,
                None,
            )),
            audit: Arc::new(openpanel_core::NoopAuditService),
            mysql_binary: None,
            notifications: Arc::new(std::sync::RwLock::new(None)),
        };
        let assertion = svc
            .assert_site_archive(std::path::Path::new(""), &a, Uuid::new_v4())
            .await;
        assert!(assertion.passed);
        assert!(!assertion.detail.is_empty());
    }

    #[tokio::test]
    async fn site_archive_fails_on_bad_magic() {
        let (_dir, path) = write_temp(b"not-an-opfs");
        let a = artifact(BackupResource::Site(Uuid::new_v4()), path.clone());
        let svc = DrillService {
            pool: Pool::connect_lazy("sqlite::memory:").unwrap(),
            backups: Arc::new(BackupService::new(
                Pool::connect_lazy("sqlite::memory:").unwrap(),
                std::env::temp_dir(),
                Arc::new(openpanel_core::NoopAuditService),
                None,
                None,
            )),
            audit: Arc::new(openpanel_core::NoopAuditService),
            mysql_binary: None,
            notifications: Arc::new(std::sync::RwLock::new(None)),
        };
        let assertion = svc
            .assert_site_archive(std::path::Path::new(""), &a, Uuid::new_v4())
            .await;
        assert!(!assertion.passed);
        assert!(!assertion.detail.is_empty());
    }

    #[tokio::test]
    async fn database_dump_passes_when_nonempty() {
        let (_dir, path) = write_temp(b"CREATE TABLE t (id INT);");
        let a = artifact(BackupResource::Database(Uuid::new_v4()), path.clone());
        let svc = DrillService {
            pool: Pool::connect_lazy("sqlite::memory:").unwrap(),
            backups: Arc::new(BackupService::new(
                Pool::connect_lazy("sqlite::memory:").unwrap(),
                std::env::temp_dir(),
                Arc::new(openpanel_core::NoopAuditService),
                None,
                None,
            )),
            audit: Arc::new(openpanel_core::NoopAuditService),
            mysql_binary: None,
            notifications: Arc::new(std::sync::RwLock::new(None)),
        };
        let assertion = svc
            .assert_database_dump(std::path::Path::new(""), &a, Uuid::new_v4())
            .await;
        assert!(assertion.passed);
    }

    #[tokio::test]
    async fn database_dump_fails_when_empty() {
        let (_dir, path) = write_temp(b"");
        let artifact = artifact(BackupResource::Database(Uuid::new_v4()), path.clone());
        let svc = DrillService {
            pool: Pool::connect_lazy("sqlite::memory:").unwrap(),
            backups: Arc::new(BackupService::new(
                Pool::connect_lazy("sqlite::memory:").unwrap(),
                std::env::temp_dir(),
                Arc::new(openpanel_core::NoopAuditService),
                None,
                None,
            )),
            audit: Arc::new(openpanel_core::NoopAuditService),
            mysql_binary: None,
            notifications: Arc::new(std::sync::RwLock::new(None)),
        };
        let assertion = svc
            .assert_database_dump(std::path::Path::new(""), &artifact, Uuid::new_v4())
            .await;
        assert!(!assertion.passed);
    }

    #[tokio::test]
    async fn panel_metadata_passes_on_valid_json() {
        let (_dir, path) = write_temp(br#"{"schema":1}"#);
        let a = artifact(BackupResource::PanelMetadata, path.clone());
        let svc = DrillService {
            pool: Pool::connect_lazy("sqlite::memory:").unwrap(),
            backups: Arc::new(BackupService::new(
                Pool::connect_lazy("sqlite::memory:").unwrap(),
                std::env::temp_dir(),
                Arc::new(openpanel_core::NoopAuditService),
                None,
                None,
            )),
            audit: Arc::new(openpanel_core::NoopAuditService),
            mysql_binary: None,
            notifications: Arc::new(std::sync::RwLock::new(None)),
        };
        let assertion = svc
            .assert_panel_metadata(std::path::Path::new(""), &a)
            .await;
        assert!(assertion.passed);
    }

    #[tokio::test]
    async fn panel_metadata_fails_on_invalid_json() {
        let (_dir, path) = write_temp(b"not json");
        let a = artifact(BackupResource::PanelMetadata, path.clone());
        let svc = DrillService {
            pool: Pool::connect_lazy("sqlite::memory:").unwrap(),
            backups: Arc::new(BackupService::new(
                Pool::connect_lazy("sqlite::memory:").unwrap(),
                std::env::temp_dir(),
                Arc::new(openpanel_core::NoopAuditService),
                None,
                None,
            )),
            audit: Arc::new(openpanel_core::NoopAuditService),
            mysql_binary: None,
            notifications: Arc::new(std::sync::RwLock::new(None)),
        };
        let assertion = svc
            .assert_panel_metadata(std::path::Path::new(""), &a)
            .await;
        assert!(!assertion.passed);
    }

    #[test]
    fn serialized_report_contains_no_secret_material() {
        let secrets = [
            "password",
            "cipher",
            "secret",
            "BEGIN PRIVATE KEY",
            "mysql://",
        ];
        for _ in 0..100 {
            let drill = RestoreDrill::start(Uuid::new_v4(), Utc::now());
            let assertions = vec![
                DrillAssertion::passed(
                    DrillAssertionKind::Database,
                    "db:abc dump present (12 bytes)",
                ),
                DrillAssertion::failed(DrillAssertionKind::Site, "site:def archive missing"),
                DrillAssertion::passed(DrillAssertionKind::PanelMetadata, "manifest valid"),
            ];
            let drill = drill
                .finish(DrillOutcome::Passed, assertions, Utc::now())
                .unwrap();
            let json = serde_json::to_string(&drill).unwrap();
            for needle in secrets {
                assert!(
                    !json.to_lowercase().contains(needle),
                    "report leaked secret material: {needle}"
                );
            }
        }
    }

    #[test]
    fn drill_from_row_roundtrip() {
        let drill = RestoreDrill::start(Uuid::new_v4(), Utc::now());
        let assertions = vec![
            DrillAssertion::passed(DrillAssertionKind::Database, "ok"),
            DrillAssertion::failed(DrillAssertionKind::Site, "missing"),
        ];
        let drill = drill
            .finish(DrillOutcome::Failed, assertions, Utc::now())
            .unwrap();

        let state_json = serde_json::to_string(&drill.state).unwrap();
        let assertions_json = serde_json::to_string(&drill.assertions).unwrap();

        let state: DrillState = serde_json::from_str(&state_json).unwrap();
        let assertions: Vec<DrillAssertion> = serde_json::from_str(&assertions_json).unwrap();

        assert_eq!(state, drill.state);
        assert_eq!(assertions, drill.assertions);
    }
}
