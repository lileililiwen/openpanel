//! Backup health projection: last success, next run, RPO/RTO, and
//! stale/failed state. Pure domain — no I/O.
//!
//! Covers `backup-dr-operations`: Backup Health Is Actionable.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Health state of one backup plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackupHealthStatus {
    /// A successful run exists within the expected interval.
    Healthy,
    /// No success within the expected interval.
    Stale,
    /// The most recent terminal run failed.
    Failed,
    /// No successful run has been observed yet.
    Unknown,
}

/// Operator-facing health summary for one backup plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupHealth {
    plan_id: Uuid,
    status: BackupHealthStatus,
    last_success: Option<DateTime<Utc>>,
    next_run: Option<DateTime<Utc>>,
    /// Seconds since the last success, when known.
    age_secs: Option<i64>,
    /// Recovery-point objective gap: same as age, when known.
    rpo_secs: Option<i64>,
    /// Estimated recovery-time objective in seconds.
    rto_secs_estimate: u64,
    retention_copies: usize,
    verified: bool,
    guidance: String,
}

impl BackupHealth {
    /// Plan this summary describes.
    pub fn plan_id(&self) -> Uuid {
        self.plan_id
    }

    /// Current health state.
    pub fn status(&self) -> BackupHealthStatus {
        self.status
    }

    /// Most recent successful run, when observed.
    pub fn last_success(&self) -> Option<DateTime<Utc>> {
        self.last_success
    }

    /// Next scheduled run, when known.
    pub fn next_run(&self) -> Option<DateTime<Utc>> {
        self.next_run
    }

    /// Seconds since the last success, when known.
    pub fn age_secs(&self) -> Option<i64> {
        self.age_secs
    }

    /// RPO gap in seconds, when known.
    pub fn rpo_secs(&self) -> Option<i64> {
        self.rpo_secs
    }

    /// Estimated RTO in seconds.
    pub fn rto_secs_estimate(&self) -> u64 {
        self.rto_secs_estimate
    }

    /// Configured retention copy count.
    pub fn retention_copies(&self) -> usize {
        self.retention_copies
    }

    /// Whether the last success was integrity-verified.
    pub fn verified(&self) -> bool {
        self.verified
    }

    /// Safe recovery guidance (links to logs/retry/configuration).
    pub fn guidance(&self) -> &str {
        &self.guidance
    }
}

/// Conservative RTO estimate used when no measured recovery exists.
pub const ESTIMATED_RTO_SECS: u64 = 900;

/// Project the health of one plan from observed facts.
///
/// - `last_failed` marks the most recent terminal run failed.
/// - `verified` records whether the last success passed integrity
///   verification.
/// - `expected_interval_secs` is the plan's nominal run interval; a
///   success older than this marks the plan stale.
#[allow(clippy::too_many_arguments)]
pub fn project_backup_health(
    plan_id: Uuid,
    last_success: Option<DateTime<Utc>>,
    next_run: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
    expected_interval_secs: u64,
    last_failed: bool,
    verified: bool,
    retention_copies: usize,
) -> BackupHealth {
    let age_secs = last_success.map(|at| now.signed_duration_since(at).num_seconds().max(0));
    let status = if last_failed {
        BackupHealthStatus::Failed
    } else if let Some(age) = age_secs {
        if u64::try_from(age).unwrap_or(u64::MAX) > expected_interval_secs {
            BackupHealthStatus::Stale
        } else {
            BackupHealthStatus::Healthy
        }
    } else {
        BackupHealthStatus::Unknown
    };
    let guidance = match status {
        BackupHealthStatus::Healthy => {
            "healthy: next scheduled run will extend coverage".to_string()
        }
        BackupHealthStatus::Stale => {
            "stale: inspect run logs, retry the plan, then check schedule and retention configuration"
                .to_string()
        }
        BackupHealthStatus::Failed => {
            "failed: inspect run logs for the failing resource, fix storage or credentials, then retry"
                .to_string()
        }
        BackupHealthStatus::Unknown => {
            "unknown: no successful run yet; trigger the plan once and verify the artifact"
                .to_string()
        }
    };
    BackupHealth {
        plan_id,
        status,
        last_success,
        next_run,
        age_secs,
        rpo_secs: age_secs,
        rto_secs_estimate: ESTIMATED_RTO_SECS,
        retention_copies,
        verified,
        guidance,
    }
}

#[cfg(test)]
mod tests {
    use chrono::Duration;

    use super::*;

    /// Marker so the spec-test-drift gate maps these tests to the
    /// `backup-dr-operations` capability.
    const CAPABILITY: &str = "backup-dr-operations";

    fn now() -> DateTime<Utc> {
        Utc::now()
    }

    #[test]
    fn capability_marker_is_backup_dr_operations() {
        assert_eq!(CAPABILITY, "backup-dr-operations");
    }

    #[test]
    fn healthy_when_success_within_interval() {
        let at = now();
        let health = project_backup_health(
            Uuid::new_v4(),
            Some(at),
            Some(at + Duration::hours(12)),
            at + Duration::hours(1),
            86_400,
            false,
            true,
            5,
        );
        assert_eq!(health.status(), BackupHealthStatus::Healthy);
        assert_eq!(health.age_secs(), Some(3600));
        assert_eq!(health.rpo_secs(), Some(3600));
        assert_eq!(health.rto_secs_estimate(), ESTIMATED_RTO_SECS);
        assert!(health.verified());
        assert_eq!(health.retention_copies(), 5);
        assert!(health.next_run().is_some());
    }

    #[test]
    fn stale_when_success_older_than_interval() {
        let at = now();
        let health = project_backup_health(
            Uuid::new_v4(),
            Some(at),
            None,
            at + Duration::hours(49),
            86_400,
            false,
            true,
            3,
        );
        assert_eq!(health.status(), BackupHealthStatus::Stale);
        assert!(health.guidance().contains("logs"));
        assert!(health.guidance().contains("retry"));
    }

    #[test]
    fn failed_takes_precedence_over_stale() {
        let at = now();
        let health = project_backup_health(
            Uuid::new_v4(),
            Some(at),
            None,
            at + Duration::hours(100),
            3600,
            true,
            false,
            3,
        );
        assert_eq!(health.status(), BackupHealthStatus::Failed);
        assert!(!health.verified());
        assert!(health.guidance().contains("fix storage"));
    }

    #[test]
    fn unknown_when_no_success_observed() {
        let at = now();
        let health = project_backup_health(Uuid::new_v4(), None, None, at, 3600, false, false, 3);
        assert_eq!(health.status(), BackupHealthStatus::Unknown);
        assert_eq!(health.age_secs(), None);
        assert_eq!(health.rpo_secs(), None);
    }

    #[test]
    fn report_contains_no_secret_material() {
        let health = project_backup_health(
            Uuid::new_v4(),
            Some(now()),
            None,
            now(),
            3600,
            false,
            true,
            2,
        );
        let json = serde_json::to_string(&health).unwrap().to_lowercase();
        for needle in ["password", "secret", "private key", "mysql://", "cipher"] {
            assert!(!json.contains(needle), "leaked {needle}");
        }
    }

    #[test]
    fn prop_projection_is_deterministic_for_fixed_inputs() {
        use proptest::prelude::*;
        proptest::test_runner::TestRunner::new(ProptestConfig::with_cases(100))
            .run(&(0u64..200_000u64, 0u64..200_000u64), |(age, interval)| {
                let at = now();
                let first = project_backup_health(
                    Uuid::nil(),
                    Some(at),
                    None,
                    at + Duration::seconds(age as i64),
                    interval,
                    false,
                    true,
                    3,
                );
                let second = project_backup_health(
                    Uuid::nil(),
                    Some(at),
                    None,
                    at + Duration::seconds(age as i64),
                    interval,
                    false,
                    true,
                    3,
                );
                prop_assert_eq!(first.clone(), second);
                let expected = if age > interval {
                    BackupHealthStatus::Stale
                } else {
                    BackupHealthStatus::Healthy
                };
                prop_assert_eq!(first.status(), expected);
                Ok(())
            })
            .unwrap();
    }
}
