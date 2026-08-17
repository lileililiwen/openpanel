//! Cron schedules, jobs, and execution history.

use std::{
    path::{Component, Path, PathBuf},
    str::FromStr,
    time::Duration,
};

use chrono::{DateTime, Utc};
use chrono_tz::Tz;
pub use scope::cron_global_default;
// Re-export the scope / quota refinement types from the
// `refine-cron-with-role-permissions` change so callers can
// import them from the `cron` module.
pub use scope::{
    CronQuota, CronScopeError, JobScope, check_quota, is_executable_allowed, is_under_owned_site,
    role_allows_scope,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

mod scope;

/// Errors caused by invalid cron data or state transitions.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum CronError {
    /// A schedule expression is invalid.
    #[error("invalid cron schedule: {0}")]
    InvalidSchedule(String),
    /// An IANA timezone name is invalid.
    #[error("unknown timezone: {0}")]
    InvalidTimezone(String),
    /// A command working directory is unsafe or not owned by the user.
    #[error("invalid working directory")]
    InvalidWorkingDirectory,
    /// A command executable must be an absolute path.
    #[error("invalid executable")]
    InvalidExecutable,
    /// A job field is invalid.
    #[error("invalid job: {0}")]
    InvalidJob(String),
    /// The run has already reached a terminal state.
    #[error("run is already terminal")]
    RunTerminal,
    /// The requested run transition is not allowed.
    #[error("invalid run transition")]
    InvalidRunTransition,
}

/// A validated five-field cron expression paired with an IANA timezone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CronSchedule {
    expression: String,
    timezone: String,
}

impl CronSchedule {
    /// Parse and validate a standard five-field cron expression.
    pub fn parse(
        expression: impl Into<String>,
        timezone: impl Into<String>,
    ) -> Result<Self, CronError> {
        let expression = expression.into();
        let timezone = timezone.into();
        if expression.split_whitespace().count() != 5 {
            return Err(CronError::InvalidSchedule(expression));
        }
        Tz::from_str(&timezone).map_err(|_| CronError::InvalidTimezone(timezone.clone()))?;
        cron::Schedule::from_str(&format!("0 {expression}"))
            .map_err(|error| CronError::InvalidSchedule(error.to_string()))?;
        Ok(Self {
            expression,
            timezone,
        })
    }

    /// Return the next scheduled UTC instant strictly after `after`.
    pub fn next_after(&self, after: DateTime<Utc>) -> Result<DateTime<Utc>, CronError> {
        let timezone = Tz::from_str(&self.timezone)
            .map_err(|_| CronError::InvalidTimezone(self.timezone.clone()))?;
        let schedule = cron::Schedule::from_str(&format!("0 {}", self.expression))
            .map_err(|error| CronError::InvalidSchedule(error.to_string()))?;
        schedule
            .after(&after.with_timezone(&timezone))
            .next()
            .map(|next| next.with_timezone(&Utc))
            .ok_or_else(|| CronError::InvalidSchedule("schedule has no future occurrence".into()))
    }

    /// The original five-field expression.
    pub fn expression(&self) -> &str {
        &self.expression
    }

    /// The IANA timezone name.
    pub fn timezone(&self) -> &str {
        &self.timezone
    }
}

/// An absolute working directory beneath one of an owner's site roots.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnedWorkingDirectory(PathBuf);

impl OwnedWorkingDirectory {
    /// Validate an absolute, traversal-free path against owned roots.
    pub fn new(path: impl AsRef<Path>, owned_roots: &[PathBuf]) -> Result<Self, CronError> {
        let path = path.as_ref();
        if !path.is_absolute()
            || path
                .components()
                .any(|part| matches!(part, Component::ParentDir))
        {
            return Err(CronError::InvalidWorkingDirectory);
        }
        if !owned_roots
            .iter()
            .any(|root| root.is_absolute() && path.starts_with(root))
        {
            return Err(CronError::InvalidWorkingDirectory);
        }
        Ok(Self(path.to_path_buf()))
    }

    /// The validated path.
    pub fn as_path(&self) -> &Path {
        &self.0
    }
}

/// Work performed by a scheduled job.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum JobKind {
    /// Execute a binary directly, never through a shell.
    Command {
        /// Absolute executable path.
        executable: PathBuf,
        /// Argument vector passed directly to the process.
        arguments: Vec<String>,
        /// Validated owner-controlled directory.
        working_directory: OwnedWorkingDirectory,
    },
    /// Send an HTTP request.
    Http {
        /// HTTP or HTTPS target.
        url: String,
        /// Uppercase HTTP method.
        method: String,
    },
}

impl JobKind {
    /// Build a validated direct command invocation.
    pub fn command(
        executable: impl Into<PathBuf>,
        arguments: Vec<String>,
        working_directory: OwnedWorkingDirectory,
    ) -> Result<Self, CronError> {
        let executable = executable.into();
        if !executable.is_absolute() || executable.as_os_str().is_empty() {
            return Err(CronError::InvalidExecutable);
        }
        Ok(Self::Command {
            executable,
            arguments,
            working_directory,
        })
    }

    /// Build a basic HTTP job.
    pub fn http(url: impl Into<String>, method: impl Into<String>) -> Result<Self, CronError> {
        let url = url.into();
        if !(url.starts_with("http://") || url.starts_with("https://")) {
            return Err(CronError::InvalidJob(
                "HTTP URL must use http or https".into(),
            ));
        }
        Ok(Self::Http {
            url,
            method: method.into().to_uppercase(),
        })
    }
}

/// Behavior when a previous run remains active.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OverlapPolicy {
    /// Skip the new occurrence.
    Skip,
}

/// A scheduled job aggregate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CronJob {
    id: Uuid,
    owner_id: Uuid,
    name: String,
    schedule: CronSchedule,
    kind: JobKind,
    timeout_secs: u64,
    overlap_policy: OverlapPolicy,
    enabled: bool,
    next_run_at: DateTime<Utc>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl CronJob {
    /// Create an enabled job and calculate its first occurrence.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: Uuid,
        owner_id: Uuid,
        name: impl Into<String>,
        schedule: CronSchedule,
        kind: JobKind,
        timeout: Duration,
        overlap_policy: OverlapPolicy,
        now: DateTime<Utc>,
    ) -> Result<Self, CronError> {
        let name = name.into();
        if name.trim().is_empty() || timeout.is_zero() || timeout.as_secs() > 86_400 {
            return Err(CronError::InvalidJob(
                "name and timeout must be valid".into(),
            ));
        }
        let next_run_at = schedule.next_after(now)?;
        Ok(Self {
            id,
            owner_id,
            name,
            schedule,
            kind,
            timeout_secs: timeout.as_secs(),
            overlap_policy,
            enabled: true,
            next_run_at,
            created_at: now,
            updated_at: now,
        })
    }

    /// Restore a persisted aggregate after storage validation.
    pub fn restore(serialized: &str) -> Result<Self, CronError> {
        serde_json::from_str(serialized).map_err(|error| CronError::InvalidJob(error.to_string()))
    }

    /// Serialize the aggregate for persistence.
    pub fn to_json(&self) -> Result<String, CronError> {
        serde_json::to_string(self).map_err(|e| CronError::InvalidJob(e.to_string()))
    }

    /// Disable future occurrences.
    pub fn disable(&mut self) {
        self.enabled = false;
        self.updated_at = Utc::now();
    }

    /// Enable and recalculate the next occurrence.
    pub fn enable(&mut self, now: DateTime<Utc>) -> Result<(), CronError> {
        self.enabled = true;
        self.next_run_at = self.schedule.next_after(now)?;
        self.updated_at = now;
        Ok(())
    }

    /// Update scheduling and execution fields.
    pub fn update(
        &mut self,
        name: String,
        schedule: CronSchedule,
        kind: JobKind,
        timeout: Duration,
        now: DateTime<Utc>,
    ) -> Result<(), CronError> {
        if name.trim().is_empty() || timeout.is_zero() || timeout.as_secs() > 86_400 {
            return Err(CronError::InvalidJob(
                "name and timeout must be valid".into(),
            ));
        }
        self.name = name;
        self.schedule = schedule;
        self.kind = kind;
        self.timeout_secs = timeout.as_secs();
        self.next_run_at = self.schedule.next_after(now)?;
        self.updated_at = now;
        Ok(())
    }

    /// Advance to the next due instant.
    pub fn advance(&mut self, after: DateTime<Utc>) -> Result<(), CronError> {
        self.next_run_at = self.schedule.next_after(after)?;
        self.updated_at = after;
        Ok(())
    }

    /// Job id.
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

    /// Schedule.
    pub fn schedule(&self) -> &CronSchedule {
        &self.schedule
    }

    /// Work definition.
    pub fn kind(&self) -> &JobKind {
        &self.kind
    }

    /// Timeout.
    pub fn timeout(&self) -> Duration {
        Duration::from_secs(self.timeout_secs)
    }

    /// Whether enabled.
    pub fn enabled(&self) -> bool {
        self.enabled
    }

    /// Next due instant.
    pub fn next_run_at(&self) -> DateTime<Utc> {
        self.next_run_at
    }
}

/// Execution lifecycle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunState {
    /// Claimed but not started.
    Leased,
    /// Currently executing.
    Running,
    /// Exited successfully.
    Succeeded,
    /// Exited unsuccessfully.
    Failed,
    /// Exceeded timeout.
    TimedOut,
    /// Suppressed by overlap policy.
    Skipped,
    /// Interrupted during shutdown or recovery.
    Interrupted,
}

impl RunState {
    fn terminal(self) -> bool {
        !matches!(self, Self::Leased | Self::Running)
    }
}

/// Durable execution history record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobRun {
    id: Uuid,
    job_id: Uuid,
    state: RunState,
    leased_at: DateTime<Utc>,
    lease_until: DateTime<Utc>,
    started_at: Option<DateTime<Utc>>,
    finished_at: Option<DateTime<Utc>>,
    exit_code: Option<i32>,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

impl JobRun {
    /// Create a leased run.
    pub fn leased(id: Uuid, job_id: Uuid, now: DateTime<Utc>, lease_until: DateTime<Utc>) -> Self {
        Self {
            id,
            job_id,
            state: RunState::Leased,
            leased_at: now,
            lease_until,
            started_at: None,
            finished_at: None,
            exit_code: None,
            stdout: vec![],
            stderr: vec![],
        }
    }

    /// Create a terminal run suppressed by overlap policy.
    pub fn skipped(id: Uuid, job_id: Uuid, now: DateTime<Utc>) -> Self {
        Self {
            id,
            job_id,
            state: RunState::Skipped,
            leased_at: now,
            lease_until: now,
            started_at: None,
            finished_at: Some(now),
            exit_code: None,
            stdout: vec![],
            stderr: vec![],
        }
    }

    /// Transition a lease to running.
    pub fn start(&mut self, now: DateTime<Utc>) -> Result<(), CronError> {
        if self.state != RunState::Leased {
            return Err(CronError::InvalidRunTransition);
        }
        self.state = RunState::Running;
        self.started_at = Some(now);
        Ok(())
    }

    /// Complete with captured output.
    pub fn complete(
        &mut self,
        now: DateTime<Utc>,
        exit_code: i32,
        stdout: Vec<u8>,
        stderr: Vec<u8>,
    ) -> Result<(), CronError> {
        self.ensure_active()?;
        self.state = if exit_code == 0 {
            RunState::Succeeded
        } else {
            RunState::Failed
        };
        self.finished_at = Some(now);
        self.exit_code = Some(exit_code);
        self.stdout = stdout;
        self.stderr = stderr;
        Ok(())
    }

    /// Mark timed out.
    pub fn timeout(&mut self, now: DateTime<Utc>) -> Result<(), CronError> {
        self.finish(RunState::TimedOut, now)
    }

    /// Mark interrupted.
    pub fn interrupt(&mut self, now: DateTime<Utc>) -> Result<(), CronError> {
        self.finish(RunState::Interrupted, now)
    }

    fn finish(&mut self, state: RunState, now: DateTime<Utc>) -> Result<(), CronError> {
        self.ensure_active()?;
        self.state = state;
        self.finished_at = Some(now);
        Ok(())
    }

    fn ensure_active(&self) -> Result<(), CronError> {
        if self.state.terminal() {
            Err(CronError::RunTerminal)
        } else {
            Ok(())
        }
    }

    /// Run id.
    pub fn id(&self) -> Uuid {
        self.id
    }

    /// Job id.
    pub fn job_id(&self) -> Uuid {
        self.job_id
    }

    /// Current state.
    pub fn state(&self) -> RunState {
        self.state
    }

    /// Lease expiry used for abandoned-run recovery.
    pub fn lease_until(&self) -> DateTime<Utc> {
        self.lease_until
    }

    /// Whether this run can no longer transition.
    pub fn is_terminal(&self) -> bool {
        self.state.terminal()
    }

    /// Captured stdout.
    pub fn stdout(&self) -> &[u8] {
        &self.stdout
    }

    /// Captured stderr.
    pub fn stderr(&self) -> &[u8] {
        &self.stderr
    }
}

#[cfg(test)]
mod tests {
    use std::{path::PathBuf, time::Duration};

    use chrono::{TimeZone, Utc};
    use proptest::prelude::*;
    use uuid::Uuid;

    use super::*;

    fn command() -> JobKind {
        JobKind::command(
            "/usr/bin/php",
            vec!["artisan".to_string()],
            OwnedWorkingDirectory::new(
                "/var/www/example.com/public_html",
                &[PathBuf::from("/var/www/example.com")],
            )
            .expect("owned working directory"),
        )
        .expect("command")
    }

    #[test]
    fn schedule_parses_five_fields_and_calculates_next_utc_occurrence() {
        let schedule = CronSchedule::parse("0 2 * * *", "Asia/Shanghai").expect("schedule");
        let after = Utc.with_ymd_and_hms(2026, 8, 9, 17, 59, 0).unwrap();
        assert_eq!(
            schedule.next_after(after).expect("next"),
            Utc.with_ymd_and_hms(2026, 8, 9, 18, 0, 0).unwrap()
        );
    }

    #[test]
    fn schedule_rejects_wrong_field_count_unknown_timezone_and_bad_ranges() {
        for (expression, timezone) in [
            ("* * * *", "UTC"),
            ("* * * * * *", "UTC"),
            ("61 * * * *", "UTC"),
            ("* * * * *", "Not/A_Zone"),
        ] {
            assert!(CronSchedule::parse(expression, timezone).is_err());
        }
    }

    #[test]
    fn schedule_handles_dst_gap_and_fold_without_repeating_an_instant() {
        let gap = CronSchedule::parse("30 2 * * *", "America/New_York").expect("gap schedule");
        let before_gap = Utc.with_ymd_and_hms(2026, 3, 8, 6, 0, 0).unwrap();
        let next = gap.next_after(before_gap).expect("after gap");
        assert!(next > before_gap);

        let fold = CronSchedule::parse("30 1 * * *", "America/New_York").expect("fold schedule");
        let before_fold = Utc.with_ymd_and_hms(2026, 11, 1, 4, 0, 0).unwrap();
        let first = fold.next_after(before_fold).expect("first fold occurrence");
        let second = fold.next_after(first).expect("second fold occurrence");
        assert!(second > first, "next occurrence must strictly advance");
    }

    #[test]
    fn working_directory_rejects_relative_parent_and_unowned_paths() {
        let roots = [PathBuf::from("/var/www/example.com")];
        for path in [
            "public_html",
            "/var/www/example.com/../other",
            "/var/www/other.example/public_html",
        ] {
            assert!(
                OwnedWorkingDirectory::new(path, &roots).is_err(),
                "accepted {path}"
            );
        }
    }

    #[test]
    fn command_rejects_empty_or_relative_executable() {
        let working = OwnedWorkingDirectory::new(
            "/var/www/example.com/public_html",
            &[PathBuf::from("/var/www/example.com")],
        )
        .expect("working directory");
        assert!(JobKind::command("", vec![], working.clone()).is_err());
        assert!(JobKind::command("php", vec![], working).is_err());
    }

    #[test]
    fn cron_job_validates_timeout_and_transitions_enablement() {
        let now = Utc.with_ymd_and_hms(2026, 8, 9, 0, 0, 0).unwrap();
        let schedule = CronSchedule::parse("*/5 * * * *", "UTC").expect("schedule");
        assert!(
            CronJob::new(
                Uuid::new_v4(),
                Uuid::new_v4(),
                "job",
                schedule.clone(),
                command(),
                Duration::ZERO,
                OverlapPolicy::Skip,
                now,
            )
            .is_err()
        );
        let mut job = CronJob::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            "job",
            schedule,
            command(),
            Duration::from_secs(60),
            OverlapPolicy::Skip,
            now,
        )
        .expect("job");
        assert!(job.enabled());
        job.disable();
        assert!(!job.enabled());
        job.enable(now).expect("enable");
        assert!(job.enabled());
        assert!(job.next_run_at() > now);
    }

    #[test]
    fn run_terminal_states_cannot_transition() {
        let now = Utc::now();
        let mut run = JobRun::leased(
            Uuid::new_v4(),
            Uuid::new_v4(),
            now,
            now + chrono::Duration::minutes(2),
        );
        run.start(now).expect("start");
        run.complete(now, 0, b"ok".to_vec(), Vec::new())
            .expect("complete");
        assert!(run.timeout(now).is_err());
        assert!(run.interrupt(now).is_err());
    }

    proptest! {
        #[test]
        fn prop_valid_interval_schedules_strictly_advance(minutes in 1u8..60) {
            let schedule = CronSchedule::parse(format!("*/{minutes} * * * *"), "UTC").unwrap();
            let after = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
            prop_assert!(schedule.next_after(after).unwrap() > after);
        }

        #[test]
        fn prop_parent_segments_never_escape_owned_root(segment in "[A-Za-z0-9_-]{0,20}") {
            let path = format!("/var/www/example.com/{segment}/../other/../../escape");
            let roots = [PathBuf::from("/var/www/example.com")];
            prop_assert!(OwnedWorkingDirectory::new(path, &roots).is_err());
        }

        #[test]
        fn prop_terminal_runs_stay_terminal(exit_code in -255i32..255) {
            let now = Utc::now();
            let mut run = JobRun::leased(Uuid::new_v4(), Uuid::new_v4(), now, now + chrono::Duration::minutes(1));
            run.start(now).unwrap();
            run.complete(now, exit_code, Vec::new(), Vec::new()).unwrap();
            prop_assert!(run.interrupt(now).is_err());
        }
    }
}
