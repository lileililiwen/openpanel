//! Preset mapping use-cases for `refine-cron-with-preset-types`.
//!
//! Pure mapping over the existing `CronService` / `BackupService` /
//! `LogService` shapes: each preset resolves to an allow-listed
//! executable + scoped workdir + compiled cycle expression, then the
//! caller invokes the unchanged `create` use-case. No scheduler,
//! overlap, quota, or storage change lives here.

use std::path::PathBuf;

use openpanel_domain::cron::{
    BackupBinding, BackupDestination, CronPresetType, CyclePicker, PresetError, preset_compile,
};

use super::service::CronInput;
use crate::backups::BackupPlanInput;
use openpanel_domain::backups::BackupResource;

/// A validated preset creation request from the web form.
#[derive(Debug, Clone)]
pub struct PresetRequest {
    /// Preset type name (`shell`, `backup-site`, ...).
    pub preset: String,
    /// Job display name.
    pub name: String,
    /// Cycle picker kind (`daily`, `n-hours`, ...).
    pub cycle: String,
    /// Cycle picker N.
    pub cycle_n: u32,
    /// Timezone.
    pub timezone: String,
    /// Working directory (command presets).
    pub working_directory: String,
    /// Backup-to destination (backup presets).
    pub backup_to: String,
    /// Credential locator (bucket / URL); empty for Local.
    pub locator: String,
    /// Retain-latest-N (backup presets).
    pub retain_latest: usize,
    /// Exclusion patterns (backup presets).
    pub exclusions: Vec<String>,
    /// Target URL (access-url preset).
    pub url: String,
    /// HTTP method (access-url preset).
    pub method: String,
    /// Scoped path argument (cut-log / backup-dir presets).
    pub target_path: String,
}

/// Compile a [`PresetRequest`] to the existing [`CronInput`].
///
/// Errors are [`PresetError`]; scope/quota/allow-list enforcement
/// stays inside `CronService::create`.
pub fn preset_to_cron_input(
    request: &PresetRequest,
    owned_roots: &[PathBuf],
    allow_list: &[String],
) -> Result<CronInput, PresetError> {
    let preset = CronPresetType::parse(&request.preset)?;
    let cycle = CyclePicker::parse(&request.cycle, request.cycle_n)?;
    let schedule = cycle.compile_expression()?;
    if preset.is_http() {
        if request.url.is_empty()
            || !(request.url.starts_with("http://") || request.url.starts_with("https://"))
        {
            return Err(PresetError::InvalidCycle(
                "access-url requires http(s) URL".into(),
            ));
        }
        return Ok(CronInput {
            name: request.name.clone(),
            schedule,
            timezone: request.timezone.clone(),
            kind: "http".into(),
            executable: None,
            arguments: vec![],
            working_directory: None,
            url: Some(request.url.clone()),
            method: Some(if request.method.is_empty() {
                "GET".into()
            } else {
                request.method.to_uppercase()
            }),
            timeout_secs: preset.default_timeout_secs(),
            overlap_policy: "skip".into(),
        });
    }
    let compiled = preset_compile(preset, &request.working_directory, owned_roots, allow_list)?;
    let mut arguments = Vec::new();
    if preset.is_backup() || preset == CronPresetType::CutLog {
        if !request.target_path.is_empty() {
            arguments.push(request.target_path.clone());
        }
        for exclusion in &request.exclusions {
            arguments.push(format!("--exclude={exclusion}"));
        }
    }
    Ok(CronInput {
        name: request.name.clone(),
        schedule,
        timezone: request.timezone.clone(),
        kind: "command".into(),
        executable: Some(compiled.executable),
        arguments,
        working_directory: Some(compiled.workdir.to_string_lossy().into_owned()),
        url: None,
        method: None,
        timeout_secs: compiled.timeout_secs,
        overlap_policy: "skip".into(),
    })
}

/// Compile the backup half of a backup preset to the existing
/// [`BackupPlanInput`]. Returns `None` for non-backup presets.
pub fn preset_to_backup_plan(
    request: &PresetRequest,
    schedule_expression: &str,
) -> Result<Option<BackupPlanInput>, PresetError> {
    let preset = CronPresetType::parse(&request.preset)?;
    if !preset.is_backup() {
        return Ok(None);
    }
    let destination = BackupDestination::parse(&request.backup_to)?;
    let binding = BackupBinding::new(
        destination,
        request.locator.clone(),
        request.retain_latest.max(1),
        request.exclusions.clone(),
    )?;
    Ok(Some(BackupPlanInput {
        name: request.name.clone(),
        resources: vec![BackupResource::PanelMetadata],
        schedule: schedule_expression.to_string(),
        timezone: request.timezone.clone(),
        retention_copies: binding.retention_copies(),
    }))
}
