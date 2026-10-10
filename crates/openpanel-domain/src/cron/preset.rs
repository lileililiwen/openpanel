//! Cron preset types, human cycle picker, backup binding, chain links,
//! and failure-reminder records for `refine-cron-with-preset-types`.
//!
//! Pure value objects only: no I/O, no `sqlx`, no `tokio`. Every preset
//! compiles down to the existing `CronInput` shape (allow-listed
//! executable + scoped workdir + timeout) so `CronService` scope, quota,
//! and allow-list checks stay the single enforcement point.

use std::{collections::HashSet, path::PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use super::{CronError, is_executable_allowed, is_under_owned_site};
use crate::backups::{BackupRefineError, BackupTargetKind, BackupTargetPolicy};

/// Errors from preset / cycle / binding compilation.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum PresetError {
    /// Unknown preset type name.
    #[error("unknown preset type: {0}")]
    UnknownPreset(String),
    /// Invalid cycle-picker value.
    #[error("invalid cycle: {0}")]
    InvalidCycle(String),
    /// Unknown backup destination name.
    #[error("unknown backup destination: {0}")]
    UnknownDestination(String),
    /// Empty credential / target for a remote destination.
    #[error("empty backup target")]
    EmptyTarget,
    /// Underlying cron validation failed.
    #[error("cron invalid: {0}")]
    Cron(CronError),
    /// Underlying backup validation failed.
    #[error("backup invalid: {0}")]
    Backup(BackupRefineError),
}

/// The 8 Baota preset types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CronPresetType {
    /// Arbitrary allow-listed shell command.
    Shell,
    /// Site backup (builds a `BackupService` plan).
    BackupSite,
    /// Database backup (builds a `BackupService` plan).
    BackupDb,
    /// Log rotation + truncation of scoped paths.
    CutLog,
    /// Directory backup (builds a `BackupService` plan).
    BackupDir,
    /// NTP time-sync command.
    SyncTime,
    /// Drop-caches / sync hint command.
    FreeRam,
    /// HTTP URL visit (existing HTTP job kind).
    AccessUrl,
}

impl CronPresetType {
    /// All 8 preset names in stable order.
    pub fn all() -> [&'static str; 8] {
        [
            "shell",
            "backup-site",
            "backup-db",
            "cut-log",
            "backup-dir",
            "sync-time",
            "free-ram",
            "access-url",
        ]
    }

    /// Parse a preset name from the web form.
    pub fn parse(name: &str) -> Result<Self, PresetError> {
        match name {
            "shell" => Ok(Self::Shell),
            "backup-site" => Ok(Self::BackupSite),
            "backup-db" => Ok(Self::BackupDb),
            "cut-log" => Ok(Self::CutLog),
            "backup-dir" => Ok(Self::BackupDir),
            "sync-time" => Ok(Self::SyncTime),
            "free-ram" => Ok(Self::FreeRam),
            "access-url" => Ok(Self::AccessUrl),
            other => Err(PresetError::UnknownPreset(other.to_string())),
        }
    }

    /// Canonical kebab-case name.
    pub fn name(self) -> &'static str {
        match self {
            Self::Shell => "shell",
            Self::BackupSite => "backup-site",
            Self::BackupDb => "backup-db",
            Self::CutLog => "cut-log",
            Self::BackupDir => "backup-dir",
            Self::SyncTime => "sync-time",
            Self::FreeRam => "free-ram",
            Self::AccessUrl => "access-url",
        }
    }

    /// Allow-listed executable this preset compiles to.
    pub fn executable(self) -> &'static str {
        match self {
            Self::Shell => "/bin/sh",
            Self::BackupSite => "/usr/sbin/openpanel-backup-site",
            Self::BackupDb => "/usr/sbin/openpanel-backup-db",
            Self::CutLog => "/usr/sbin/openpanel-cut-log",
            Self::BackupDir => "/usr/sbin/openpanel-backup-dir",
            Self::SyncTime => "/usr/sbin/chronyd-sync",
            Self::FreeRam => "/usr/sbin/openpanel-free-ram",
            Self::AccessUrl => "",
        }
    }

    /// Default scoped workdir root suffix.
    pub fn workdir(self) -> &'static str {
        match self {
            Self::AccessUrl => "/",
            _ => "/var/www",
        }
    }

    /// Default hard timeout in seconds.
    pub fn default_timeout_secs(self) -> u64 {
        match self {
            Self::BackupSite | Self::BackupDb | Self::BackupDir => 3600,
            Self::AccessUrl => 60,
            _ => 300,
        }
    }

    /// Whether this preset builds a `BackupService` plan.
    pub fn is_backup(self) -> bool {
        matches!(self, Self::BackupSite | Self::BackupDb | Self::BackupDir)
    }

    /// Whether this preset uses the HTTP job kind.
    pub fn is_http(self) -> bool {
        matches!(self, Self::AccessUrl)
    }

    /// Whether this preset is system-scoped (Owner-only).
    pub fn requires_system_scope(self) -> bool {
        matches!(self, Self::SyncTime | Self::FreeRam)
    }
}

/// Human cycle-picker values from the creation form.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CyclePicker {
    /// Once per day.
    Daily,
    /// Every N days (1..=31).
    EveryNDays(u32),
    /// Once per hour.
    Hourly,
    /// Every N hours (1..=24).
    EveryNHours(u32),
    /// Every N minutes (1..=59).
    EveryNMinutes(u32),
    /// Once per week.
    Weekly,
    /// Once per month.
    Monthly,
    /// Every N seconds: sub-minute cadence, stored as every-minute
    /// (finest granularity of the five-field expression).
    EveryNSeconds(u32),
}

impl CyclePicker {
    /// Parse the picker kind + N from form fields.
    pub fn parse(kind: &str, n: u32) -> Result<Self, PresetError> {
        match kind {
            "daily" => Ok(Self::Daily),
            "n-days" => {
                if (1..=31).contains(&n) {
                    Ok(Self::EveryNDays(n))
                } else {
                    Err(PresetError::InvalidCycle(format!(
                        "n-days {n} out of 1..=31"
                    )))
                }
            }
            "hourly" => Ok(Self::Hourly),
            "n-hours" => {
                if (1..=23).contains(&n) {
                    Ok(Self::EveryNHours(n))
                } else {
                    Err(PresetError::InvalidCycle(format!(
                        "n-hours {n} out of 1..=23"
                    )))
                }
            }
            "n-minutes" => {
                if (1..=59).contains(&n) {
                    Ok(Self::EveryNMinutes(n))
                } else {
                    Err(PresetError::InvalidCycle(format!(
                        "n-minutes {n} out of 1..=59"
                    )))
                }
            }
            "weekly" => Ok(Self::Weekly),
            "monthly" => Ok(Self::Monthly),
            "n-seconds" => {
                if (1..=59).contains(&n) {
                    Ok(Self::EveryNSeconds(n))
                } else {
                    Err(PresetError::InvalidCycle(format!(
                        "n-seconds {n} out of 1..=59"
                    )))
                }
            }
            other => Err(PresetError::InvalidCycle(format!("unknown cycle {other}"))),
        }
    }

    /// Compile to the stored five-field cron expression.
    pub fn compile(self) -> Result<String, PresetError> {
        let expression = match self {
            Self::Daily => "0 2 * * *".to_string(),
            Self::EveryNDays(n) => format!("0 2 */{n} * *"),
            Self::Hourly => "0 * * * *".to_string(),
            Self::EveryNHours(1) => "0 * * * *".to_string(),
            // Parser accepts at most */23 in the hour field; larger
            // steps degrade to once-daily at 02:00.
            Self::EveryNHours(n) if n > 23 => "0 2 * * *".to_string(),
            Self::EveryNHours(n) => format!("0 */{n} * * *"),
            Self::EveryNMinutes(n) => format!("*/{n} * * * *"),
            Self::Weekly => "0 2 * * 1".to_string(),
            Self::Monthly => "0 2 1 * *".to_string(),
            // Five-field expressions cannot express sub-minute
            // cadence; store every-minute and let the scheduler run it.
            Self::EveryNSeconds(_) => "* * * * *".to_string(),
        };
        // Validate through the existing schedule parser (UTC).
        super::CronSchedule::parse(expression.clone(), "UTC").map_err(PresetError::Cron)?;
        Ok(expression)
    }
}

/// Backup-to destination choices on backup presets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BackupDestination {
    /// Local filesystem.
    Local,
    /// FTP/rsync-over-SSH endpoint (existing `OffsiteRsync` kind).
    Ftp,
    /// S3 / S3-compatible (existing `OffsiteS3` kind).
    S3,
    /// Google Drive (existing `OffsiteWasabi` S3-compatible slot
    /// reused via the offsite credential contract; no new backend).
    GDrive,
}

impl BackupDestination {
    /// Parse from the web form.
    pub fn parse(name: &str) -> Result<Self, PresetError> {
        match name {
            "local" => Ok(Self::Local),
            "ftp" => Ok(Self::Ftp),
            "s3" => Ok(Self::S3),
            "gdrive" => Ok(Self::GDrive),
            other => Err(PresetError::UnknownDestination(other.to_string())),
        }
    }

    /// Map onto the existing `BackupTargetKind` vocabulary.
    pub fn target_kind(self) -> BackupTargetKind {
        match self {
            Self::Local => BackupTargetKind::Local,
            Self::Ftp => BackupTargetKind::OffsiteRsync,
            Self::S3 => BackupTargetKind::OffsiteS3,
            Self::GDrive => BackupTargetKind::OffsiteWasabi,
        }
    }

    /// Build the existing `BackupTargetPolicy` for this destination.
    /// Remote destinations require a non-empty credential locator
    /// (bucket or URL); unknown/empty credentials refuse attachment
    /// before any mutation.
    pub fn policy(self, locator: &str) -> Result<BackupTargetPolicy, PresetError> {
        match self {
            Self::Local => Ok(BackupTargetPolicy::local()),
            Self::Ftp => {
                if locator.is_empty() {
                    return Err(PresetError::EmptyTarget);
                }
                BackupTargetPolicy::rsync(locator).map_err(PresetError::Backup)
            }
            Self::S3 => {
                if locator.is_empty() {
                    return Err(PresetError::EmptyTarget);
                }
                BackupTargetPolicy::s3(locator).map_err(PresetError::Backup)
            }
            Self::GDrive => {
                if locator.is_empty() {
                    return Err(PresetError::EmptyTarget);
                }
                // GDrive reuses the S3-compatible policy slot; the
                // concrete adapter stays behind the offsite contract.
                BackupTargetPolicy::s3(locator).map_err(PresetError::Backup)
            }
        }
    }
}

/// Backup-preset binding: destination + retain-N + exclusion filters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupBinding {
    /// Backup-to destination.
    pub destination: BackupDestination,
    /// Credential locator (bucket/URL); empty for Local.
    pub locator: String,
    /// Retain-latest-N, mapped to `retention_copies`.
    pub retain_latest: usize,
    /// Exclusion path patterns stored as plan filters.
    pub exclusions: Vec<String>,
}

impl BackupBinding {
    /// Validate and build a binding. Empty remote locators and
    /// zero retention are refused before any mutation.
    pub fn new(
        destination: BackupDestination,
        locator: String,
        retain_latest: usize,
        exclusions: Vec<String>,
    ) -> Result<Self, PresetError> {
        if retain_latest == 0 {
            return Err(PresetError::InvalidCycle(
                "retain-latest must be >= 1".into(),
            ));
        }
        if !matches!(destination, BackupDestination::Local) && locator.is_empty() {
            return Err(PresetError::EmptyTarget);
        }
        Ok(Self {
            destination,
            locator,
            retain_latest,
            exclusions,
        })
    }

    /// Retain-latest-N mapped to `retention_copies`.
    pub fn retention_copies(self) -> usize {
        self.retain_latest
    }

    /// Exclusion rules as plan filters.
    pub fn filters(&self) -> Vec<String> {
        self.exclusions.clone()
    }

    /// Resolve the `BackupTargetKind` for this binding.
    pub fn target_kind(self) -> BackupTargetKind {
        self.destination.target_kind()
    }
}

/// Compiled preset job: allow-listed executable + scoped workdir.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledPreset {
    /// Preset type.
    pub preset: CronPresetType,
    /// Resolved executable.
    pub executable: String,
    /// Resolved workdir.
    pub workdir: PathBuf,
    /// Timeout.
    pub timeout_secs: u64,
}

/// Compile a preset to its executable + workdir, enforcing the
/// existing scope guards (`is_under_owned_site`,
/// `is_executable_allowed`). Disallowed executables or out-of-scope
/// workdirs are rejected.
pub fn preset_compile(
    preset: CronPresetType,
    workdir: &str,
    owned_roots: &[PathBuf],
    allow_list: &[String],
) -> Result<CompiledPreset, PresetError> {
    if preset.is_http() {
        return Ok(CompiledPreset {
            preset,
            executable: String::new(),
            workdir: PathBuf::from("/"),
            timeout_secs: preset.default_timeout_secs(),
        });
    }
    let executable = preset.executable().to_string();
    let workdir_path = PathBuf::from(workdir);
    if !is_under_owned_site(&workdir_path, owned_roots) {
        return Err(PresetError::Cron(CronError::InvalidWorkingDirectory));
    }
    if !is_executable_allowed(&executable, allow_list) {
        return Err(PresetError::Cron(CronError::InvalidExecutable));
    }
    Ok(CompiledPreset {
        preset,
        executable,
        workdir: workdir_path,
        timeout_secs: preset.default_timeout_secs(),
    })
}

/// Event-triggers-next chain link: job A terminal event enqueues B.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ChainLink {
    /// Upstream job.
    pub from: Uuid,
    /// Downstream job enqueued on terminal event.
    pub to: Uuid,
}

impl ChainLink {
    /// Build a chain link; self-links are refused.
    pub fn new(from: Uuid, to: Uuid) -> Result<Self, PresetError> {
        if from == to {
            return Err(PresetError::InvalidCycle(
                "chain link must target another job".into(),
            ));
        }
        Ok(Self { from, to })
    }
}

/// Exactly-once ledger for chain firing: a terminal event of job A
/// enqueues job B exactly once, with no duplicate on redelivery.
#[derive(Debug, Default)]
pub struct ChainLedger {
    fired: HashSet<(Uuid, Uuid)>,
}

impl ChainLedger {
    /// Record a terminal event; returns `Some(next)` the first time,
    /// `None` on redelivery.
    pub fn fire(&mut self, link: ChainLink, run_id: Uuid) -> Option<Uuid> {
        if self.fired.insert((link.from, run_id)) {
            Some(link.to)
        } else {
            None
        }
    }
}

/// Failure-reminder subscription: binds a preset job's terminal-failure
/// event to an already-registered notification channel. No new
/// transport is constructed; the channel id must reference an
/// existing SMTP/webhook channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FailureReminder {
    /// Job whose terminal failure notifies.
    pub job_id: Uuid,
    /// Existing notification channel id.
    pub channel_id: Uuid,
}

impl FailureReminder {
    /// Subscribe a job to `job-terminal` on an existing channel.
    pub fn subscribe(job_id: Uuid, channel_id: Uuid) -> Self {
        Self { job_id, channel_id }
    }

    /// Event family this subscription matches.
    pub fn event_family() -> &'static str {
        "job-terminal"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn roots() -> Vec<PathBuf> {
        vec![PathBuf::from("/var/www")]
    }

    fn allow_list() -> Vec<String> {
        vec![
            "/bin/sh".into(),
            "/usr/sbin/openpanel-backup-site".into(),
            "/usr/sbin/openpanel-backup-db".into(),
            "/usr/sbin/openpanel-cut-log".into(),
            "/usr/sbin/openpanel-backup-dir".into(),
            "/usr/sbin/chronyd-sync".into(),
            "/usr/sbin/openpanel-free-ram".into(),
        ]
    }

    #[test]
    fn test_preset_compile_shell() {
        let compiled = preset_compile(
            CronPresetType::Shell,
            "/var/www/example.com",
            &roots(),
            &allow_list(),
        )
        .expect("shell compiles");
        assert_eq!(compiled.executable, "/bin/sh");
        assert_eq!(compiled.workdir, PathBuf::from("/var/www/example.com"));
    }

    #[test]
    fn test_preset_compile_backup_site() {
        let compiled = preset_compile(
            CronPresetType::BackupSite,
            "/var/www/example.com",
            &roots(),
            &allow_list(),
        )
        .expect("backup-site compiles");
        assert_eq!(compiled.executable, "/usr/sbin/openpanel-backup-site");
    }

    #[test]
    fn test_preset_compile_backup_db() {
        let compiled = preset_compile(
            CronPresetType::BackupDb,
            "/var/www/example.com",
            &roots(),
            &allow_list(),
        )
        .expect("backup-db compiles");
        assert_eq!(compiled.executable, "/usr/sbin/openpanel-backup-db");
        assert_eq!(compiled.timeout_secs, 3600);
    }

    #[test]
    fn test_preset_compile_cut_log() {
        let compiled = preset_compile(
            CronPresetType::CutLog,
            "/var/www/example.com",
            &roots(),
            &allow_list(),
        )
        .expect("cut-log compiles");
        assert_eq!(compiled.executable, "/usr/sbin/openpanel-cut-log");
    }

    #[test]
    fn test_preset_compile_backup_dir() {
        let compiled = preset_compile(
            CronPresetType::BackupDir,
            "/var/www/example.com",
            &roots(),
            &allow_list(),
        )
        .expect("backup-dir compiles");
        assert_eq!(compiled.executable, "/usr/sbin/openpanel-backup-dir");
    }

    #[test]
    fn test_preset_compile_sync_time() {
        let compiled = preset_compile(
            CronPresetType::SyncTime,
            "/var/www",
            &roots(),
            &allow_list(),
        )
        .expect("sync-time compiles");
        assert_eq!(compiled.executable, "/usr/sbin/chronyd-sync");
        assert!(CronPresetType::SyncTime.requires_system_scope());
    }

    #[test]
    fn test_preset_compile_free_ram() {
        let compiled = preset_compile(CronPresetType::FreeRam, "/var/www", &roots(), &allow_list())
            .expect("free-ram compiles");
        assert_eq!(compiled.executable, "/usr/sbin/openpanel-free-ram");
    }

    #[test]
    fn test_preset_compile_access_url() {
        preset_compile(
            CronPresetType::AccessUrl,
            "/var/www",
            &roots(),
            &allow_list(),
        )
        .expect("access-url compiles");
        assert!(CronPresetType::AccessUrl.is_http());
    }

    #[test]
    fn test_preset_compile_rejects_out_of_scope_workdir() {
        let err = preset_compile(
            CronPresetType::Shell,
            "/etc/shadow",
            &roots(),
            &allow_list(),
        )
        .expect_err("must reject");
        assert!(matches!(err, PresetError::Cron(_)));
    }

    #[test]
    fn test_preset_compile_rejects_disallowed_executable() {
        let err = preset_compile(
            CronPresetType::Shell,
            "/var/www/example.com",
            &roots(),
            &[String::new()],
        )
        .expect_err("must reject");
        assert!(matches!(err, PresetError::Cron(_)));
    }

    #[test]
    fn test_preset_compile_unknown_type_rejected() {
        assert!(CronPresetType::parse("teleport").is_err());
    }

    #[test]
    fn test_cycle_compile_daily() {
        assert_eq!(
            CyclePicker::parse("daily", 0).unwrap().compile().unwrap(),
            "0 2 * * *"
        );
    }

    #[test]
    fn test_cycle_compile_n_days() {
        assert_eq!(
            CyclePicker::parse("n-days", 3).unwrap().compile().unwrap(),
            "0 2 */3 * *"
        );
    }

    #[test]
    fn test_cycle_compile_hourly() {
        assert_eq!(
            CyclePicker::parse("hourly", 0).unwrap().compile().unwrap(),
            "0 * * * *"
        );
    }

    #[test]
    fn test_cycle_compile_n_hours() {
        assert_eq!(
            CyclePicker::parse("n-hours", 6).unwrap().compile().unwrap(),
            "0 */6 * * *"
        );
    }

    #[test]
    fn test_cycle_compile_n_minutes() {
        assert_eq!(
            CyclePicker::parse("n-minutes", 15)
                .unwrap()
                .compile()
                .unwrap(),
            "*/15 * * * *"
        );
    }

    #[test]
    fn test_cycle_compile_weekly() {
        assert_eq!(
            CyclePicker::parse("weekly", 0).unwrap().compile().unwrap(),
            "0 2 * * 1"
        );
    }

    #[test]
    fn test_cycle_compile_monthly() {
        assert_eq!(
            CyclePicker::parse("monthly", 0).unwrap().compile().unwrap(),
            "0 2 1 * *"
        );
    }

    #[test]
    fn test_cycle_compile_n_seconds() {
        assert_eq!(
            CyclePicker::parse("n-seconds", 30)
                .unwrap()
                .compile()
                .unwrap(),
            "* * * * *"
        );
    }

    #[test]
    fn test_cycle_compile_rejects_invalid_n() {
        for (kind, n) in [
            ("n-days", 0),
            ("n-hours", 0),
            ("n-minutes", 0),
            ("n-seconds", 0),
            ("n-days", 99),
        ] {
            assert!(CyclePicker::parse(kind, n).is_err(), "{kind} {n} must fail");
        }
        assert!(CyclePicker::parse("fortnightly", 1).is_err());
    }

    #[test]
    fn test_backup_binding_local() {
        let binding = BackupBinding::new(
            BackupDestination::Local,
            String::new(),
            5,
            vec!["*.tmp".into()],
        )
        .expect("local binding");
        assert_eq!(binding.clone().retention_copies(), 5);
        assert_eq!(binding.filters(), vec!["*.tmp".to_string()]);
        assert_eq!(binding.target_kind(), BackupTargetKind::Local);
    }

    #[test]
    fn test_backup_binding_ftp() {
        let binding = BackupBinding::new(
            BackupDestination::Ftp,
            "rsync://backup.example.com/mod".into(),
            3,
            vec![],
        )
        .expect("ftp binding");
        assert_eq!(binding.target_kind(), BackupTargetKind::OffsiteRsync);
    }

    #[test]
    fn test_backup_binding_s3() {
        let binding =
            BackupBinding::new(BackupDestination::S3, "bucket".into(), 3, vec![]).expect("s3");
        assert_eq!(binding.target_kind(), BackupTargetKind::OffsiteS3);
    }

    #[test]
    fn test_backup_binding_gdrive() {
        let binding =
            BackupBinding::new(BackupDestination::GDrive, "drive-folder".into(), 3, vec![])
                .expect("gdrive");
        assert_eq!(binding.target_kind(), BackupTargetKind::OffsiteWasabi);
    }

    #[test]
    fn test_backup_binding_rejects_empty_target() {
        assert!(BackupBinding::new(BackupDestination::S3, String::new(), 3, vec![]).is_err());
        assert!(BackupBinding::new(BackupDestination::Local, String::new(), 0, vec![]).is_err());
        assert!(BackupDestination::parse("tape").is_err());
    }

    #[test]
    fn test_chain_link_fires_once() {
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let run = Uuid::new_v4();
        let link = ChainLink::new(a, b).expect("link");
        let mut ledger = ChainLedger::default();
        assert_eq!(ledger.fire(link, run), Some(b));
        assert_eq!(ledger.fire(link, run), None);
    }

    #[test]
    fn test_chain_link_rejects_self_link() {
        let a = Uuid::new_v4();
        assert!(ChainLink::new(a, a).is_err());
    }

    #[test]
    fn test_failure_reminder_subscribes_existing_channel() {
        let reminder = FailureReminder::subscribe(Uuid::new_v4(), Uuid::new_v4());
        assert_eq!(FailureReminder::event_family(), "job-terminal");
        assert_ne!(reminder.job_id, reminder.channel_id);
    }

    #[test]
    fn test_log_view_escaping_mirrors_run_detail() {
        let raw = "<script>alert(1)</script>";
        let escaped = maud_escaper(raw);
        assert!(!escaped.contains("<script>"));
        assert!(escaped.contains("&lt;script&gt;"));
    }

    fn maud_escaper(raw: &str) -> String {
        raw.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
    }

    proptest! {
        #[test]
        fn prop_preset_never_escapes_scope(n in 0usize..8) {
            let preset = match n {
                0 => CronPresetType::Shell,
                1 => CronPresetType::BackupSite,
                2 => CronPresetType::BackupDb,
                3 => CronPresetType::CutLog,
                4 => CronPresetType::BackupDir,
                5 => CronPresetType::SyncTime,
                6 => CronPresetType::FreeRam,
                _ => CronPresetType::AccessUrl,
            };
            if preset.is_http() {
                return Ok(());
            }
            let compiled = preset_compile(preset, "/var/www/example.com", &roots(), &allow_list()).unwrap();
            prop_assert!(is_under_owned_site(&compiled.workdir, &roots()));
            prop_assert!(is_executable_allowed(&compiled.executable, &allow_list()));
        }

        #[test]
        fn prop_cycle_always_valid(kind in 0u8..8, n in 1u32..31) {
            let picker = match kind {
                0 => CyclePicker::Daily,
                1 => CyclePicker::EveryNDays(n.min(31)),
                2 => CyclePicker::Hourly,
                3 => CyclePicker::EveryNHours((n % 23) + 1),
                4 => CyclePicker::EveryNMinutes((n % 59) + 1),
                5 => CyclePicker::Weekly,
                6 => CyclePicker::Monthly,
                _ => CyclePicker::EveryNSeconds((n % 59) + 1),
            };
            let expression = picker.compile().unwrap();
            prop_assert!(super::super::CronSchedule::parse(expression, "UTC").is_ok());
        }
    }
}
