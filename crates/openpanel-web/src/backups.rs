//! Browser backup plans, runs, and restore progress.

use axum::{
    Form,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
};
use maud::html;
use openpanel_app::backups::BackupPlanInput;
use openpanel_domain::{Role, backups::BackupResource};
use serde::Deserialize;

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
    let content = html! {h1{"Backups"} a href="/backups/new"{"Create backup plan"} h2{"Plans"} ul{@for plan in plans{li{(plan.name()) " — " (plan.schedule())}}} h2{"Runs"} ul{@for run in runs{li{(format!("{:?}",run.state()))}}}};
    state
        .render_shell(&user, &csrf, "/backups", content)
        .await
        .into_response()
}
/// Render plan creation form.
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
/// Browser plan fields.
pub struct PlanForm {
    _csrf: String,
    name: String,
    schedule: String,
    timezone: String,
    retention_copies: usize,
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
