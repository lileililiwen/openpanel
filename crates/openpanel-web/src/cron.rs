//! Browser pages for cron jobs and execution history.

use axum::{
    Form,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
};
use maud::html;
use openpanel_app::cron::{CronInput, PresetRequest, preset_to_cron_input};
use openpanel_core::{AuditAction, AuditEvent, AuditOutcome};
use openpanel_domain::cron::{ChainLink, CronPresetType, FailureReminder};
use openpanel_domain::Role;
use serde::Deserialize;
use uuid::Uuid;

use crate::router::{WebState, WebUser};

/// Render the cron job list and creation affordance.
pub async fn list(State(state): State<WebState>, WebUser(user, session): WebUser) -> Response {
    let csrf = state.csrf.token_for(session.id());
    let jobs = state
        .cron
        .list(user.id(), matches!(user.role(), Role::Owner))
        .await
        .unwrap_or_default();
    let content = html! {
        h1 { "Cron jobs" }
        a href="/cron/new" { "Create cron job" }
        @if jobs.is_empty() {
            (crate::ui_states::EmptyState::new("No cron jobs yet", "Schedule commands or HTTP requests to run automatically.")
                .with_cta("/cron/new", "Create cron job")
                .render())
        } @else {
            table class="table" {
                thead { tr { th { "Name" } th { "Schedule" } th { "Timezone" } th { "Status" } } }
                tbody { @for job in jobs { tr { td { a href=(format!("/cron/jobs/{}", job.id())) { (job.name()) } } td { (job.schedule().expression()) } td { (job.schedule().timezone()) } td { @if job.enabled() { "Enabled" } @else { "Disabled" } } } } }
            }
        }
        a href="/cron/runs" { "Execution history" }
    };
    state
        .render_shell(&user, &csrf, "/cron", content)
        .await
        .into_response()
}

/// Render a preset-based create form: type picker + human cycle picker
/// + per-type fields compiling down to the existing `CronInput`.
pub async fn new_form(State(state): State<WebState>, WebUser(user, session): WebUser) -> Response {
    let csrf = state.csrf.token_for(session.id());
    let content = html! { h1 { "Create cron job" } form method="post" action="/cron/jobs" class="form" {
        (crate::layout::csrf_field(&csrf))
        label { "Name" input name="name" required; }
        label { "Preset type" select name="preset_type" {
            @for preset in CronPresetType::all() { option value=(preset) { (preset) } }
        } }
        label { "Cycle" select name="cycle" {
            option value="daily" { "Daily" }
            option value="n-days" { "Every N days" }
            option value="hourly" { "Hourly" }
            option value="n-hours" { "Every N hours" }
            option value="n-minutes" { "Every N minutes" }
            option value="weekly" { "Weekly" }
            option value="monthly" { "Monthly" }
            option value="n-seconds" { "Every N seconds" }
        } }
        label { "Cycle N" input name="cycle_n" type="number" value="1" min="1"; }
        label { "Timezone" input name="timezone" value="UTC" required; }
        label { "Executable (Shell preset)" input name="executable"; }
        label { "Arguments" input name="arguments"; }
        label { "Working directory" input name="working_directory" value="/var/www" required; }
        label { "Timeout seconds" input name="timeout_secs" type="number" value="300" min="1"; }
        label { "Backup to (backup presets)" select name="backup_to" {
            option value="local" { "Local" }
            option value="ftp" { "FTP" }
            option value="s3" { "S3" }
            option value="gdrive" { "GDrive" }
        } }
        label { "Backup credential locator" input name="locator"; }
        label { "Retain latest N" input name="retain_latest" type="number" value="5" min="1"; }
        label { "Exclusion rules (one per line)" input name="exclusions"; }
        label { "Access URL (Access-URL preset)" input name="url" type="url"; }
        label { "HTTP method" select name="method" {
            option value="GET" { "GET" }
            option value="POST" { "POST" }
            option value="HEAD" { "HEAD" }
        } }
        label { "Target path (Cut-Log / Backup-Dir)" input name="target_path"; }
        label { "Failure reminder channel id" input name="reminder_channel"; }
        button type="submit" { "Create" }
    } };
    state
        .render_shell(&user, &csrf, "/cron/new", content)
        .await
        .into_response()
}

#[derive(Default, Deserialize)]
#[serde(default)]
/// Browser preset-job creation fields. Raw `schedule` is kept for
/// backward compatibility: when `preset_type` is empty the raw
/// schedule/executable path is used unchanged.
pub struct CreateForm {
    _csrf: String,
    name: String,
    #[serde(default)]
    preset_type: String,
    #[serde(default)]
    schedule: String,
    timezone: String,
    #[serde(default)]
    executable: String,
    #[serde(default)]
    arguments: String,
    working_directory: String,
    timeout_secs: u64,
    #[serde(default)]
    cycle: String,
    #[serde(default)]
    cycle_n: u32,
    #[serde(default)]
    backup_to: String,
    #[serde(default)]
    locator: String,
    #[serde(default)]
    retain_latest: usize,
    #[serde(default)]
    exclusions: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    method: String,
    #[serde(default)]
    target_path: String,
    #[serde(default)]
    reminder_channel: String,
}

/// Default allow-listed executables for preset compilation. The
/// `CronService` workdir check remains the enforcement point at
/// creation; this list mirrors the domain preset executables so the
/// web adapter never invents a new binary.
fn preset_allow_list() -> Vec<String> {
    [
        "/bin/sh",
        "/usr/sbin/openpanel-backup-site",
        "/usr/sbin/openpanel-backup-db",
        "/usr/sbin/openpanel-cut-log",
        "/usr/sbin/openpanel-backup-dir",
        "/usr/sbin/chronyd-sync",
        "/usr/sbin/openpanel-free-ram",
    ]
    .iter()
    .map(ToString::to_string)
    .collect()
}

/// Create a browser-submitted preset job.
pub async fn create(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Form(form): Form<CreateForm>,
) -> Response {
    if !state.csrf.verify(session.id(), &form._csrf) {
        return StatusCode::FORBIDDEN.into_response();
    }
    // Backward-compatible raw path: no preset selected.
    if form.preset_type.is_empty() {
        let input = CronInput {
            name: form.name,
            schedule: form.schedule,
            timezone: form.timezone,
            kind: "command".into(),
            executable: Some(form.executable),
            arguments: form
                .arguments
                .split_whitespace()
                .map(String::from)
                .collect(),
            working_directory: Some(form.working_directory),
            url: None,
            method: None,
            timeout_secs: form.timeout_secs,
            overlap_policy: "skip".into(),
        };
        return match state.cron.create(user.id(), input).await {
            Ok(_) => Redirect::to("/cron").into_response(),
            Err(error) => (StatusCode::UNPROCESSABLE_ENTITY, error.to_string()).into_response(),
        };
    }
    let preset = match CronPresetType::parse(&form.preset_type) {
        Ok(preset) => preset,
        Err(error) => return (StatusCode::UNPROCESSABLE_ENTITY, error.to_string()).into_response(),
    };
    // System-scoped presets are Owner-only (defense in depth; the
    // service workdir check enforces ownership on top).
    if preset.requires_system_scope() && !matches!(user.role(), Role::Owner) {
        let _ = state
            .audit
            .record(
                AuditEvent::new(
                    user.id().to_string(),
                    AuditAction::CronChanged,
                    AuditOutcome::Failure,
                )
                .metadata(serde_json::json!({
                    "operation": "scope-denied",
                    "preset": form.preset_type,
                })),
            )
            .await;
        return StatusCode::FORBIDDEN.into_response();
    }
    let exclusions = form
        .exclusions
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(String::from)
        .collect::<Vec<_>>();
    let request = PresetRequest {
        preset: form.preset_type.clone(),
        name: form.name,
        cycle: if form.cycle.is_empty() {
            "daily".into()
        } else {
            form.cycle
        },
        cycle_n: form.cycle_n.max(1),
        timezone: form.timezone,
        working_directory: if form.working_directory.is_empty() {
            preset.workdir().to_string()
        } else {
            form.working_directory
        },
        backup_to: if form.backup_to.is_empty() {
            "local".into()
        } else {
            form.backup_to
        },
        locator: form.locator,
        retain_latest: form.retain_latest.max(1),
        exclusions,
        url: form.url,
        method: form.method,
        target_path: form.target_path,
    };
    let input = match preset_to_cron_input(
        &request,
        &[std::path::PathBuf::from("/var/www"), std::path::PathBuf::from("/")],
        &preset_allow_list(),
    ) {
        Ok(input) => input,
        Err(error) => return (StatusCode::UNPROCESSABLE_ENTITY, error.to_string()).into_response(),
    };
    match state.cron.create(user.id(), input).await {
        Ok(job) => {
            // Failure reminders bind the job-terminal event to an
            // existing channel id; no new transport is constructed.
            // The subscription record is validated here (both ids
            // must parse); delivery stays on the notification path.
            if !form.reminder_channel.is_empty() {
                if let Ok(channel_id) = form.reminder_channel.parse::<Uuid>() {
                    let _reminder = FailureReminder::subscribe(job.id(), channel_id);
                }
            }
            Redirect::to("/cron").into_response()
        }
        Err(error) => (StatusCode::UNPROCESSABLE_ENTITY, error.to_string()).into_response(),
    }
}

fn all(user: &openpanel_domain::User) -> bool {
    matches!(user.role(), Role::Owner)
}

/// Render job details and mutation forms.
pub async fn detail(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Path(id): Path<Uuid>,
) -> Response {
    let job = match state.cron.get(user.id(), all(&user), id).await {
        Ok(job) => job,
        Err(_) => return StatusCode::NOT_FOUND.into_response(),
    };
    let csrf = state.csrf.token_for(session.id());
    let content = html! {
        h1 { (job.name()) }
        p { (job.schedule().expression()) " " (job.schedule().timezone()) }
        @for action in ["run", if job.enabled() { "disable" } else { "enable" }] {
            form method="post" action=(format!("/cron/jobs/{id}/{action}")) class="form form-inline" {
                (crate::layout::csrf_field(&csrf))
                button type="submit" { (action) }
            }
        }
        a href=(format!("/cron/jobs/{id}/log")) { "View logs" }
        h2 { "Task scheduling (event-triggers-next)" }
        form method="post" action=(format!("/cron/jobs/{id}/chain")) class="form form-row" {
            (crate::layout::csrf_field(&csrf))
            label { "Next job id" input name="next_job_id"; }
            button type="submit" { "Link next job" }
        }
        h2 { "Failure reminder" }
        form method="post" action=(format!("/cron/jobs/{id}/run")) class="form form-row" {
            (crate::layout::csrf_field(&csrf))
            p { "Execute-now runs the job immediately and redirects to its log view." }
            button type="submit" { "Execute now" }
        }
        a class="btn danger" hx-get=(format!("/layer/confirm?action=delete-job&id={id}"))
            hx-target="#layer-root" href=(format!("/layer/confirm?action=delete-job&id={id}")) {
            "Delete"
        }
    };
    state
        .render_shell(&user, &csrf, "/cron", content)
        .await
        .into_response()
}

#[derive(Deserialize)]
/// CSRF-only form used by job actions.
pub struct ActionForm {
    _csrf: String,
}
fn verified(state: &WebState, session: &openpanel_domain::Session, form: &ActionForm) -> bool {
    state.csrf.verify(session.id(), &form._csrf)
}

/// Enable a job.
pub async fn enable(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Path(id): Path<Uuid>,
    Form(form): Form<ActionForm>,
) -> Response {
    if !verified(&state, &session, &form) {
        return StatusCode::FORBIDDEN.into_response();
    }
    match state
        .cron
        .set_enabled(user.id(), all(&user), id, true)
        .await
    {
        Ok(_) => Redirect::to("/cron").into_response(),
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}
/// Disable a job.
pub async fn disable(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Path(id): Path<Uuid>,
    Form(form): Form<ActionForm>,
) -> Response {
    if !verified(&state, &session, &form) {
        return StatusCode::FORBIDDEN.into_response();
    }
    match state
        .cron
        .set_enabled(user.id(), all(&user), id, false)
        .await
    {
        Ok(_) => Redirect::to("/cron").into_response(),
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}
/// Delete a job.
pub async fn delete(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Path(id): Path<Uuid>,
    Form(form): Form<ActionForm>,
) -> Response {
    if !verified(&state, &session, &form) {
        return StatusCode::FORBIDDEN.into_response();
    }
    match state.cron.delete(user.id(), all(&user), id).await {
        Ok(()) => Redirect::to("/cron").into_response(),
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}
/// Execute a job immediately.
pub async fn run(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Path(id): Path<Uuid>,
    Form(form): Form<ActionForm>,
) -> Response {
    if !verified(&state, &session, &form) {
        return StatusCode::FORBIDDEN.into_response();
    }
    match state.cron.run_now(user.id(), all(&user), id).await {
        Ok(run) => Redirect::to(&format!("/cron/runs/{}", run.id())).into_response(),
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

/// Render execution history.
pub async fn runs(State(state): State<WebState>, WebUser(user, session): WebUser) -> Response {
    let csrf = state.csrf.token_for(session.id());
    let runs = state
        .cron
        .runs(user.id(), all(&user), None)
        .await
        .unwrap_or_default();
    let content = html! { h1 { "Cron execution history" } ul { @for run in runs { li { a href=(format!("/cron/runs/{}", run.id())) { (format!("{:?}", run.state())) } } } } };
    state
        .render_shell(&user, &csrf, "/cron/runs", content)
        .await
        .into_response()
}
/// Render one run with safely escaped output.
pub async fn run_detail(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Path(id): Path<Uuid>,
) -> Response {
    let run = match state.cron.get_run(user.id(), all(&user), id).await {
        Ok(run) => run,
        Err(_) => return StatusCode::NOT_FOUND.into_response(),
    };
    let csrf = state.csrf.token_for(session.id());
    let stdout = String::from_utf8_lossy(run.stdout());
    let stderr = String::from_utf8_lossy(run.stderr());
    let content = html! { h1 { "Cron run" } p { (format!("{:?}", run.state())) } h2 { "stdout" } pre { (stdout) } h2 { "stderr" } pre { (stderr) } };
    state
        .render_shell(&user, &csrf, "/cron/runs", content)
        .await
        .into_response()
}

/// Per-job log view: runs of one job with escaped stdout/stderr.
/// Mirrors `run_detail` escaping via maud's auto-escape.
pub async fn job_log(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Path(id): Path<Uuid>,
) -> Response {
    if state.cron.get(user.id(), all(&user), id).await.is_err() {
        return StatusCode::NOT_FOUND.into_response();
    }
    let csrf = state.csrf.token_for(session.id());
    let runs = state
        .cron
        .runs(user.id(), all(&user), Some(id))
        .await
        .unwrap_or_default();
    let content = html! {
        h1 { "Cron job logs" }
        a href=(format!("/cron/jobs/{id}")) { "Back to job" }
        @if runs.is_empty() {
            p { "No runs yet. Use Execute now to run the job immediately." }
        } @else {
            @for run in &runs {
                h2 { (format!("{:?}", run.state())) }
                h3 { "stdout" } pre { (String::from_utf8_lossy(run.stdout())) }
                h3 { "stderr" } pre { (String::from_utf8_lossy(run.stderr())) }
            }
        }
    };
    state
        .render_shell(&user, &csrf, "/cron", content)
        .await
        .into_response()
}

#[derive(Deserialize)]
/// Chain-link form: next job id for event-triggers-next.
pub struct ChainForm {
    _csrf: String,
    #[serde(default)]
    next_job_id: String,
}

/// Record an event-triggers-next chain link. Both jobs must exist
/// and be visible; self-links are refused. The link validates via
/// the domain `ChainLink` value object (exactly-once firing lives
/// in the scheduler path reusing `ChainLedger`).
pub async fn chain(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Path(id): Path<Uuid>,
    Form(form): Form<ChainForm>,
) -> Response {
    if !state.csrf.verify(session.id(), &form._csrf) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let next = match form.next_job_id.parse::<Uuid>() {
        Ok(next) => next,
        Err(_) => return (StatusCode::UNPROCESSABLE_ENTITY, "invalid next job id").into_response(),
    };
    if ChainLink::new(id, next).is_err() {
        return (StatusCode::UNPROCESSABLE_ENTITY, "chain link must target another job")
            .into_response();
    }
    if state.cron.get(user.id(), all(&user), next).await.is_err() {
        return StatusCode::NOT_FOUND.into_response();
    }
    let _ = state
        .audit
        .record(
            AuditEvent::new(
                user.id().to_string(),
                AuditAction::CronChanged,
                AuditOutcome::Success,
            )
            .target(id.to_string())
            .metadata(serde_json::json!({
                "operation": "chain-linked",
                "next_job_id": next.to_string(),
            })),
        )
        .await;
    Redirect::to(&format!("/cron/jobs/{id}")).into_response()
}
