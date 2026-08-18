//! Browser pages for cron jobs and execution history.

use axum::{
    Form,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
};
use maud::html;
use openpanel_app::cron::CronInput;
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
            table {
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

/// Render a basic create form. API clients may use the richer JSON endpoint.
pub async fn new_form(State(state): State<WebState>, WebUser(user, session): WebUser) -> Response {
    let csrf = state.csrf.token_for(session.id());
    let content = html! { h1 { "Create cron job" } form method="post" action="/cron/jobs" class="form" {
        (crate::layout::csrf_field(&csrf))
        label { "Name" input name="name" required; }
        label { "Schedule" input name="schedule" value="0 * * * *" required; }
        label { "Timezone" input name="timezone" value="UTC" required; }
        label { "Executable" input name="executable" required; }
        label { "Arguments" input name="arguments"; }
        label { "Working directory" input name="working_directory" required; }
        label { "Timeout seconds" input name="timeout_secs" type="number" value="60" min="1"; }
        button type="submit" { "Create" }
    } };
    state
        .render_shell(&user, &csrf, "/cron/new", content)
        .await
        .into_response()
}

#[derive(Default, Deserialize)]
#[serde(default)]
/// Browser command-job creation fields.
pub struct CreateForm {
    _csrf: String,
    name: String,
    schedule: String,
    timezone: String,
    executable: String,
    #[serde(default)]
    arguments: String,
    working_directory: String,
    timeout_secs: u64,
}

/// Create a browser-submitted command job.
pub async fn create(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Form(form): Form<CreateForm>,
) -> Response {
    if !state.csrf.verify(session.id(), &form._csrf) {
        return StatusCode::FORBIDDEN.into_response();
    }
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
    match state.cron.create(user.id(), input).await {
        Ok(_) => Redirect::to("/cron").into_response(),
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
