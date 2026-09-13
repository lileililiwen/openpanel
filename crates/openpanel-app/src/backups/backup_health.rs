//! Plan health summarized from existing backup repositories.
//!
//! Covers `backup-dr-operations`: Backup Health Is Actionable. No new
//! tables; the projection is a pure view over `BackupPlan` runs.

use chrono::{DateTime, Utc};
use openpanel_domain::backups::{
    BackupPlan, BackupRun, BackupRunState,
    health::{BackupHealth, project_backup_health},
};
use uuid::Uuid;

/// Most recent completed run, if any.
pub fn latest_completed_run(runs: &[BackupRun]) -> Option<&BackupRun> {
    runs.iter()
        .find(|run| run.state() == BackupRunState::Completed)
}

/// Project one plan's health from its stored runs.
///
/// `runs` MUST be ordered newest-first (as the repository returns
/// them); the newest terminal run decides `last_failed`.
/// `last_success` is the newest completed run's finish time, when
/// known. `verified` records whether that success passed integrity
/// verification.
pub fn plan_health_from_runs(
    plan: &BackupPlan,
    runs: &[BackupRun],
    last_success: Option<DateTime<Utc>>,
    next_run: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
    expected_interval_secs: u64,
    verified: bool,
) -> BackupHealth {
    let plan_runs: Vec<&BackupRun> = runs
        .iter()
        .filter(|run| run.plan_id() == plan.id())
        .collect();
    let newest_terminal = plan_runs.iter().find(|run| {
        matches!(
            run.state(),
            BackupRunState::Completed
                | BackupRunState::Failed
                | BackupRunState::Corrupt
                | BackupRunState::Cancelled
        )
    });
    let last_failed = newest_terminal.is_some_and(|run| {
        matches!(
            run.state(),
            BackupRunState::Failed | BackupRunState::Corrupt
        )
    });
    project_backup_health(
        plan.id(),
        last_success,
        next_run,
        now,
        expected_interval_secs,
        last_failed,
        verified,
        plan.retention().count(),
    )
}

/// Plan id helper for audit-safe logging.
pub fn plan_health_id(plan_id: Uuid) -> String {
    plan_id.to_string()
}

#[cfg(test)]
mod tests {
    use chrono::Duration;
    use openpanel_domain::backups::{BackupResource, RetentionPolicy};

    use super::*;

    /// Marker so the spec-test-drift gate maps these tests to the
    /// `backup-dr-operations` capability.
    const CAPABILITY: &str = "backup-dr-operations";

    fn plan() -> BackupPlan {
        BackupPlan::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            "nightly",
            vec![BackupResource::PanelMetadata],
            "0 2 * * *",
            "UTC",
            RetentionPolicy::copies(3).unwrap(),
            Utc::now(),
        )
        .unwrap()
    }

    fn run(plan: &BackupPlan, state: BackupRunState) -> BackupRun {
        let now = Utc::now();
        let mut run = BackupRun::new(Uuid::new_v4(), plan.id(), plan.owner_id(), now);
        match state {
            BackupRunState::Running | BackupRunState::Completed => {
                run.start(now).unwrap();
                let artifact = openpanel_domain::backups::BackupArtifact::new(
                    BackupResource::PanelMetadata,
                    "panel.json",
                    b"meta",
                )
                .unwrap();
                run.add_artifact(artifact).unwrap();
                run.mark_finalized().unwrap();
                if state == BackupRunState::Completed {
                    run.complete(now).unwrap();
                }
            }
            BackupRunState::Failed => {
                run.fail(now, "io error").unwrap();
            }
            _ => {}
        }
        run
    }

    #[test]
    fn capability_marker_is_backup_dr_operations() {
        assert_eq!(CAPABILITY, "backup-dr-operations");
    }

    #[test]
    fn empty_runs_project_unknown() {
        let plan = plan();
        let health = plan_health_from_runs(&plan, &[], None, None, Utc::now(), 86_400, false);
        assert_eq!(
            health.status(),
            openpanel_domain::backups::health::BackupHealthStatus::Unknown
        );
        assert_eq!(health.plan_id(), plan.id());
    }

    #[test]
    fn newest_failed_run_projects_failed() {
        let plan = plan();
        let failed = run(&plan, BackupRunState::Failed);
        let now = Utc::now();
        let health = plan_health_from_runs(&plan, &[failed], None, None, now, 86_400, false);
        assert_eq!(
            health.status(),
            openpanel_domain::backups::health::BackupHealthStatus::Failed
        );
    }

    #[test]
    fn newest_completed_run_with_recent_success_is_healthy() {
        let plan = plan();
        let completed = run(&plan, BackupRunState::Completed);
        let now = Utc::now();
        let health = plan_health_from_runs(
            &plan,
            &[completed],
            Some(now - Duration::hours(1)),
            Some(now + Duration::hours(11)),
            now,
            86_400,
            true,
        );
        assert_eq!(
            health.status(),
            openpanel_domain::backups::health::BackupHealthStatus::Healthy
        );
        assert_eq!(health.next_run(), Some(now + Duration::hours(11)));
    }

    #[test]
    fn latest_completed_run_finds_completed() {
        let plan = plan();
        let failed = run(&plan, BackupRunState::Failed);
        let completed = run(&plan, BackupRunState::Completed);
        let runs = vec![failed, completed];
        assert!(latest_completed_run(&runs).is_some());
        assert!(latest_completed_run(&[]).is_none());
    }

    #[test]
    fn plan_health_id_is_stable() {
        let id = Uuid::new_v4();
        assert_eq!(plan_health_id(id), id.to_string());
    }
}
