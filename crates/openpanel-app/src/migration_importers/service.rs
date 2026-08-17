//! Migration application service: orchestrates preview, run, and
//! rollback for the importer drivers.

use std::sync::Arc;

use chrono::{Duration, Utc};
use openpanel_core::{AuditAction, AuditEvent, AuditOutcome, AuditService};
use openpanel_domain::{
    DriverKind, ImportedResource, MigrationDriver, MigrationError, MigrationPlan,
    MigrationRepository, MigrationRun, MigrationRunId, MigrationRunStatus, TranslationLog,
    TranslationLogEntry, TranslationOutcome,
};
use uuid::Uuid;

/// TTL for a confirmed plan before it expires (design: 60s).
pub const PLAN_TTL: Duration = Duration::seconds(60);
/// Rollback window: a run may be undone within 24h of commit.
pub const ROLLBACK_WINDOW: Duration = Duration::hours(24);

/// Migration application service.
#[derive(Clone)]
pub struct MigrationService {
    repo: Arc<dyn MigrationRepository>,
    audit: Arc<dyn AuditService>,
}

impl MigrationService {
    /// Build a service over the given repository.
    pub fn new(repo: Arc<dyn MigrationRepository>, audit: Arc<dyn AuditService>) -> Self {
        Self { repo, audit }
    }

    /// Preview a source bundle with a driver. When `driver_hint` is
    /// `Some`, the driver's `sniff` MUST confirm the hint; otherwise
    /// the service returns `UnknownSource`.
    pub async fn preview<D>(
        &self,
        driver: &D,
        source: &D::Source,
        driver_hint: Option<DriverKind>,
        actor: &str,
    ) -> Result<MigrationPlan, MigrationError>
    where
        D: MigrationDriver,
    {
        let detected = driver.sniff(source).ok_or(MigrationError::UnknownSource(
            "no driver recognized the source",
        ))?;
        if let Some(hint) = driver_hint
            && hint != detected
        {
            return Err(MigrationError::UnknownSource(
                "driver_hint does not match the detected format",
            ));
        }
        let plan = driver.dry_run(source).await?;
        self.audit
            .record(
                AuditEvent::new(
                    actor,
                    AuditAction::MigrationPreviewed,
                    AuditOutcome::Success,
                )
                .target(plan.plan_id().to_string())
                .metadata(serde_json::json!({
                    "driver": detected.as_str(),
                    "resources": plan.resources().len(),
                    "conflicts": plan.conflicts().len(),
                })),
            )
            .await
            .ok();
        Ok(plan)
    }

    /// Commit a confirmed plan. The plan must be within `PLAN_TTL`
    /// and must not have already been imported (idempotency guard).
    pub async fn run<D>(
        &self,
        driver: &D,
        source: &D::Source,
        plan: &MigrationPlan,
        target_owner_user_id: Uuid,
        actor: &str,
    ) -> Result<Vec<ImportedResource>, MigrationError>
    where
        D: MigrationDriver,
    {
        if plan.refuses_empty_run() {
            return Err(MigrationError::MalformedSource(
                "plan contains no importable resources".to_string(),
            ));
        }
        let now = Utc::now();
        if now > plan.created_at() + PLAN_TTL {
            return Err(MigrationError::PlanExpired(plan.plan_id()));
        }
        if let Some(existing) = self.repo.recent_runs(1).await?.first()
            && existing.plan_id() == plan.plan_id()
        {
            self.audit
                .record(
                    AuditEvent::new(
                        actor,
                        AuditAction::MigrationAlreadyImportedRejected,
                        AuditOutcome::Denied,
                    )
                    .target(existing.run_id().to_string())
                    .metadata(serde_json::json!({"plan_id": plan.plan_id().to_string()})),
                )
                .await
                .ok();
            return Err(MigrationError::AlreadyImported(existing.run_id()));
        }

        let run_id = MigrationRunId::new();
        let mut run = MigrationRun::new(
            run_id,
            plan.plan_id(),
            plan.driver(),
            target_owner_user_id,
            now,
        );
        self.repo.insert_run(&run).await?;

        let imported = match driver
            .run(source, plan, run_id, target_owner_user_id, now)
            .await
        {
            Ok(imported) => imported,
            Err(error) => {
                run.mark_failed();
                self.repo.update_run_status(&run).await?;
                self.audit
                    .record(
                        AuditEvent::new(
                            actor,
                            AuditAction::MigrationRunRolledBack,
                            AuditOutcome::Failure,
                        )
                        .target(run_id.to_string())
                        .metadata(serde_json::json!({"error": error.to_string()})),
                    )
                    .await
                    .ok();
                return Err(error);
            }
        };

        // Persist the per-resource outcomes and a redacted
        // translation log entry for each resource.
        let mut log = TranslationLog::new(run_id);
        for resource in &imported {
            self.repo.insert_imported_resource(resource).await?;
            log.push(TranslationLogEntry {
                run_id,
                kind: resource.kind,
                source_key: resource.source_key.clone(),
                outcome: TranslationOutcome::Imported,
                redacted: redact_diagnostic(&resource.source_key),
            });
        }
        for entry in log.entries() {
            self.repo.insert_log_entry(entry).await?;
        }

        run.mark_completed();
        self.repo.update_run_status(&run).await?;
        self.audit
            .record(
                AuditEvent::new(
                    actor,
                    AuditAction::MigrationRunCommitted,
                    AuditOutcome::Success,
                )
                .target(run_id.to_string())
                .metadata(serde_json::json!({
                    "plan_id": plan.plan_id().to_string(),
                    "resources": imported.len(),
                })),
            )
            .await
            .ok();
        Ok(imported)
    }

    /// Roll back a committed run within `ROLLBACK_WINDOW`. Deletes
    /// each imported resource in reverse insertion order and marks
    /// the run `RolledBack`.
    pub async fn rollback<D>(
        &self,
        driver: &D,
        run_id: MigrationRunId,
        actor: &str,
    ) -> Result<Vec<ImportedResource>, MigrationError>
    where
        D: MigrationDriver,
    {
        let runs = self.repo.recent_runs(10).await?;
        let run = runs
            .iter()
            .find(|r| r.run_id() == run_id)
            .cloned()
            .ok_or(MigrationError::Persistence("run not found".to_string()))?;
        if !matches!(run.status(), MigrationRunStatus::Completed) {
            return Err(MigrationError::Persistence(format!(
                "run {run_id} is not completed"
            )));
        }
        let now = Utc::now();
        if now > run.confirmed_at() + ROLLBACK_WINDOW {
            return Err(MigrationError::RollbackWindowExpired(run_id));
        }

        let mut imported = self.repo.imported_resources(run_id).await?;
        let rolled_back = driver.rollback(&imported, run_id, now).await?;

        for resource in &mut imported {
            resource.mark_rolled_back();
            self.repo
                .mark_imported_resource_rolled_back(resource)
                .await?;
        }

        let mut run = run;
        run.mark_rolled_back();
        self.repo.update_run_status(&run).await?;
        self.audit
            .record(
                AuditEvent::new(
                    actor,
                    AuditAction::MigrationRollbackCompleted,
                    AuditOutcome::Success,
                )
                .target(run_id.to_string())
                .metadata(serde_json::json!({"resources": rolled_back.len()})),
            )
            .await
            .ok();
        Ok(rolled_back)
    }

    /// List recent import runs within the retention window.
    pub async fn recent_runs(&self, limit: i64) -> Result<Vec<MigrationRun>, MigrationError> {
        self.repo.recent_runs(limit).await
    }

    /// Load the imported resources for a run.
    pub async fn imported_resources(
        &self,
        run_id: MigrationRunId,
    ) -> Result<Vec<ImportedResource>, MigrationError> {
        self.repo.imported_resources(run_id).await
    }
}

/// Redact a diagnostic so no secret leaks into the persistent log.
/// Keeps only the natural key — never payload bytes or credentials.
fn redact_diagnostic(source_key: &str) -> String {
    let mut key = source_key.chars().take(255).collect::<String>();
    if key.is_empty() {
        key = "<unnamed>".to_string();
    }
    format!("imported {key}")
}

#[cfg(test)]
mod tests {
    use openpanel_domain::PlannedResource;

    use super::*;

    #[test]
    fn redact_keeps_key_only() {
        let diag = redact_diagnostic("example.com");
        assert_eq!(diag, "imported example.com");
    }

    #[test]
    fn plan_expiry_uses_ttl() {
        assert_eq!(PLAN_TTL, Duration::seconds(60));
        assert_eq!(ROLLBACK_WINDOW, Duration::hours(24));
    }

    #[test]
    fn planned_resource_round_trips() {
        let resource = PlannedResource::new(
            openpanel_domain::ImportedResourceKind::Site,
            "x.com",
            "vhost",
            8,
        )
        .unwrap();
        assert_eq!(resource.bytes, 8);
        assert_eq!(resource.source_key, "x.com");
    }
}
