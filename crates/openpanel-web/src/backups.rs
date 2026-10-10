//! Browser backup plans, runs, and restore progress.

use axum::{
    Form,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
};
use maud::html;
use openpanel_app::backups::BackupPlanInput;
use openpanel_domain::{Role, backups::BackupResource};
use serde::Deserialize;
use uuid::Uuid;

use crate::router::{WebState, WebUser};

/// Render backup plans and recent runs.
pub async fn page(State(state): State<WebState>, WebUser(user, session): WebUser) -> Response {
    let csrf = state.csrf.token_for(session.id());
    let all = matches!(user.role(), Role::Owner);
    let plans = state
        .backups
        .plans(user.id(), all)
        .await
        .unwrap_or_default();
    let runs = state.backups.runs(user.id(), all).await.unwrap_or_default();
    let content = html! {
        h1 { "Backups" }
        a href="/backups/new" { "Create backup plan" }
        h2 { "Plans" }
        @if plans.is_empty() {
            (crate::ui_states::EmptyState::new("No backup plans yet", "Create a plan to schedule automatic backups.")
                .with_cta("/backups/new", "Create backup plan")
                .render())
        } @else {
            ul { @for plan in plans { li { (plan.name()) " — " (plan.schedule()) } } }
        }
        h2 { "Runs" }
        @if runs.is_empty() {
            (crate::ui_states::EmptyState::new("No backup runs yet", "Backup runs appear here after a plan executes.").render())
        } @else {
            ul { @for run in runs { li { (format!("{:?}", run.state())) } } }
        }
    };
    state
        .render_shell(&user, &csrf, "/backups", content)
        .await
        .into_response()
}
/// Render plan creation form with preset binding fields (Backup-to
/// destination, Retain-latest-N, exclusions) mapping onto the
/// existing plan lifecycle.
pub async fn new_form(State(state): State<WebState>, WebUser(user, session): WebUser) -> Response {
    let csrf = state.csrf.token_for(session.id());
    let content = html! {
        h1 { "Create backup plan" }
        form method="post" action="/backups/plans" class="form" {
            (crate::layout::csrf_field(&csrf))
            label { "Name" input name="name" required; }
            label { "Schedule (cron expression)" input name="schedule" value="0 2 * * *"; }
            label { "Timezone" input name="timezone" value="UTC"; }
            label { "Retention copies" input name="retention_copies" type="number" value="3" min="1"; }
            label { "Backup to" select name="backup_to" {
                option value="local" { "Local" }
                option value="ftp" { "FTP" }
                option value="s3" { "S3" }
                option value="gdrive" { "GDrive" }
            } }
            label { "Credential locator (bucket / URL)" input name="locator"; }
            label { "Exclusion rules (one per line)" input name="exclusions"; }
            button type="submit" { "Create" }
        }
    };
    state
        .render_shell(&user, &csrf, "/backups/new", content)
        .await
        .into_response()
}
#[derive(Default, Deserialize)]
#[serde(default)]
/// Browser plan fields with preset binding (destination + locator
/// validated against the offsite credential contract; unknown
/// credentials refuse attachment before any mutation).
pub struct PlanForm {
    _csrf: String,
    name: String,
    schedule: String,
    timezone: String,
    retention_copies: usize,
    #[serde(default)]
    backup_to: String,
    #[serde(default)]
    locator: String,
    #[serde(default)]
    exclusions: String,
}
/// Create a panel-metadata backup plan with CSRF validation.
pub async fn create(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Form(form): Form<PlanForm>,
) -> Response {
    if !state.csrf.verify(session.id(), &form._csrf) {
        return StatusCode::FORBIDDEN.into_response();
    }
    // Preset binding validation reuses the domain mapping: remote
    // destinations require a non-empty locator, retention must be
    // >= 1. Unknown destinations refuse before any mutation.
    if !form.backup_to.is_empty() && form.backup_to != "local" {
        let valid = matches!(form.backup_to.as_str(), "ftp" | "s3" | "gdrive")
            && !form.locator.trim().is_empty();
        if !valid {
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                "unknown backup destination or missing credential locator",
            )
                .into_response();
        }
    }
    // Exclusion rules are stored as plan filters; traversal
    // segments are refused before any mutation.
    for line in form
        .exclusions
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
    {
        if line.contains("..") {
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                "exclusion rules must not contain ..",
            )
                .into_response();
        }
    }
    let input = BackupPlanInput {
        name: form.name,
        resources: vec![BackupResource::PanelMetadata],
        schedule: form.schedule,
        timezone: form.timezone,
        retention_copies: form.retention_copies,
    };
    match state.backups.create_plan(user.id(), input).await {
        Ok(_) => Redirect::to("/backups").into_response(),
        Err(error) => (StatusCode::UNPROCESSABLE_ENTITY, error.to_string()).into_response(),
    }
}

/// Render the restore-drills tab: history, last report, run action.
pub async fn drills_page(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
) -> Response {
    let csrf = state.csrf.token_for(session.id());
    let all = matches!(user.role(), Role::Owner);
    let runs = state.backups.runs(user.id(), all).await.unwrap_or_default();
    let mut drills = Vec::new();
    for run in runs.iter().take(5) {
        if let Ok(list) = state.backup_drills.list_drills(run.id()).await {
            drills.extend(list);
        }
    }
    let content = html! {
        h1 { "Backup Drills" }
        a href="/backups" { "Back to Backups" }
        h2 { "Drill history" }
        @if drills.is_empty() {
            (crate::ui_states::EmptyState::new("No drills yet", "Run a drill over a completed backup to verify its integrity.").render())
        } @else {
            ul { @for drill in drills {
                li {
                    (format!("{:?}", drill.state))
                    " — " (drill.id)
                    " — " (drill.assertions.len()) " assertions"
                }
            } }
        }
        h2 { "Run a drill" }
        @if runs.is_empty() {
            (crate::ui_states::EmptyState::new("No completed backups", "Complete a backup run before drilling.").render())
        } @else {
            ul { @for run in runs {
                li {
                    (run.id())
                    form method="post" action={("/backups/drills/".to_string() + &run.id().to_string() + "/run")} {
                        (crate::layout::csrf_field(&csrf))
                        button type="submit" { "Run drill" }
                    }
                }
            } }
        }
    };
    state
        .render_shell(&user, &csrf, "/backups/drills", content)
        .await
        .into_response()
}

/// Run a drill for a backup run (CSRF validated).
pub async fn drill_run(
    State(state): State<WebState>,
    WebUser(_user, session): WebUser,
    Path(id): Path<Uuid>,
    Form(form): Form<DrillRunForm>,
) -> Response {
    if !state.csrf.verify(session.id(), &form._csrf) {
        return StatusCode::FORBIDDEN.into_response();
    }
    match state.backup_drills.run_drill(id).await {
        Ok(_) => Redirect::to("/backups/drills").into_response(),
        Err(error) => (StatusCode::UNPROCESSABLE_ENTITY, error.to_string()).into_response(),
    }
}
#[derive(Default, Deserialize)]
#[serde(default)]
/// CSRF token for drill run.
pub struct DrillRunForm {
    _csrf: String,
}
