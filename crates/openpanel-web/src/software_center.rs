//! Owner-only Software Center pages and plan confirmation forms.

use axum::{
    Form,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use maud::html;
use serde::Deserialize;

use crate::router::{WebState, WebUser};

/// Render the curated catalog and bounded job history.
pub async fn page(State(state): State<WebState>, WebUser(user, session): WebUser) -> Response {
    let catalog = match state.software_center.catalog(user.role()) {
        Ok(value) => value,
        Err(_) => return StatusCode::FORBIDDEN.into_response(),
    };
    let jobs = state
        .software_center
        .jobs(user.role())
        .await
        .unwrap_or_default();
    let inventory = state
        .software_center
        .inventory(user.role())
        .await
        .unwrap_or_default();
    let csrf = state.csrf.token_for(session.id());
    let content = html! {
        h1 { "Software Center" }
        p { "Curated system components and one-click applications" }
        section {
            h2 { "Catalog" }
            ul {
                @for entry in catalog {
                    li {
                        strong { (entry.name) }
                        " — " (entry.license)
                        @if matches!(entry.kind, openpanel_app::software_center::CatalogKind::SystemComponent) {
                            @let component_state = inventory.iter().find(|item| item.id == entry.id).map(|item| item.state.as_str()).unwrap_or("unsupported");
                            span { " — " (component_state) }
                            @match component_state {
                                "available" => {
                                    form method="post" action=(format!("/software/components/{}/preview", entry.id)) {
                                        input type="hidden" name="_csrf" value=(csrf);
                                        button { "Review install" }
                                    }
                                },
                                "externally_managed" => {
                                    form method="post" action=(format!("/software/components/{}/adopt/preview", entry.id)) {
                                        input type="hidden" name="_csrf" value=(csrf);
                                        button { "Review adoption" }
                                    }
                                },
                                "panel_managed" => {
                                    form method="post" action=(format!("/software/components/{}/update/preview", entry.id)) {
                                        input type="hidden" name="_csrf" value=(csrf);
                                        button { "Review update" }
                                    }
                                    form method="post" action=(format!("/software/components/{}/remove/preview", entry.id)) {
                                        input type="hidden" name="_csrf" value=(csrf);
                                        button { "Review removal" }
                                    }
                                },
                                _ => {},
                            }
                        } @else if entry.lifecycle_state == "available" {
                            form method="post" action="/software/applications/preview" {
                                input type="hidden" name="_csrf" value=(csrf);
                                input type="hidden" name="application" value=(entry.id);
                                label { "Domain " input name="domain" required; }
                                label { "PHP " select name="php_version" {
                                    option value="8.3" { "8.3" }
                                    option value="8.4" { "8.4" }
                                } }
                                input type="hidden" name="locale" value="en_US";
                                button { "Deploy application" }
                            }
                        } @else {
                            span { " — deployment adapter unavailable on this host" }
                        }
                    }
                }
            }
        }
        section {
            h2 { "Recent jobs" }
            ul { @for job in jobs { li {
                (job.state) " — " (job.id)
                @if matches!(job.state.as_str(), "queued" | "running" | "validating") {
                    form method="post" action=(format!("/software/jobs/{}/cancel", job.id)) {
                        input type="hidden" name="_csrf" value=(csrf);
                        button { "Cancel at safe checkpoint" }
                    }
                } @else if job.state == "interrupted" {
                    form method="post" action=(format!("/software/jobs/{}/retry", job.id)) {
                        input type="hidden" name="_csrf" value=(csrf);
                        button { "Review retry" }
                    }
                    form method="post" action=(format!("/software/jobs/{}/rollback", job.id)) {
                        input type="hidden" name="_csrf" value=(csrf);
                        button { "Run supported rollback" }
                    }
                }
            } } }
        }
    };
    state
        .render_shell(&user, &csrf, "/software", content)
        .await
        .into_response()
}

/// Run recipe-supported rollback for an interrupted component installation.
pub async fn rollback(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Path(id): Path<uuid::Uuid>,
    Form(form): Form<PreviewForm>,
) -> Response {
    if !state.csrf.verify(session.id(), &form._csrf) {
        return StatusCode::FORBIDDEN.into_response();
    }
    match state
        .software_center
        .rollback_interrupted(user.id(), user.role(), id)
        .await
    {
        Ok(_) => StatusCode::OK.into_response(),
        Err(_) => StatusCode::CONFLICT.into_response(),
    }
}

/// Generate and render a fresh confirmation for an interrupted job retry.
pub async fn retry(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Path(id): Path<uuid::Uuid>,
    Form(form): Form<PreviewForm>,
) -> Response {
    if !state.csrf.verify(session.id(), &form._csrf) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let preview = match state
        .software_center
        .retry_preview(user.id(), user.role(), id)
        .await
    {
        Ok(value) => value,
        Err(_) => return StatusCode::CONFLICT.into_response(),
    };
    let csrf = state.csrf.token_for(session.id());
    let content = html! {
        h1 { "Review interrupted transaction retry" }
        p { "Packages: " (preview.packages.join(", ")) }
        code { (preview.plan_digest) }
        form method="post" action=(format!("/software/plans/{}/execute", preview.plan_digest)) {
            input type="hidden" name="_csrf" value=(csrf);
            input type="hidden" name="confirmation_token" value=(preview.confirmation_token);
            button { "Confirm retry" }
        }
    };
    state
        .render_shell(&user, &csrf, "/software", content)
        .await
        .into_response()
}

/// Request cancellation for an active job after CSRF validation.
pub async fn cancel(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Path(id): Path<uuid::Uuid>,
    Form(form): Form<PreviewForm>,
) -> Response {
    if !state.csrf.verify(session.id(), &form._csrf) {
        return StatusCode::FORBIDDEN.into_response();
    }
    match state
        .software_center
        .cancel(user.id(), user.role(), id)
        .await
    {
        Ok(_) => StatusCode::ACCEPTED.into_response(),
        Err(_) => StatusCode::CONFLICT.into_response(),
    }
}

/// Render a digest-bound adoption, update, or removal confirmation.
pub async fn preview_component_action(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Path((id, action)): Path<(String, openpanel_app::software_center::ComponentAction)>,
    Form(form): Form<PreviewForm>,
) -> Response {
    if !state.csrf.verify(session.id(), &form._csrf) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let preview = match state
        .software_center
        .preview_component(user.id(), user.role(), &id, action)
        .await
    {
        Ok(value) => value,
        Err(_) => return StatusCode::UNPROCESSABLE_ENTITY.into_response(),
    };
    let csrf = state.csrf.token_for(session.id());
    let content = html! {
        h1 { "Review software transaction" }
        p { "Action: " (format!("{action:?}")) }
        p { "Affected services: " (preview.affected_services.join(", ")) }
        code { (preview.plan.digest()) }
        form method="post" action=(format!("/software/plans/{}/execute", preview.plan.digest())) {
            input type="hidden" name="_csrf" value=(csrf);
            input type="hidden" name="confirmation_token" value=(preview.confirmation_token);
            button { "Confirm transaction" }
        }
    };
    state
        .render_shell(&user, &csrf, "/software", content)
        .await
        .into_response()
}

/// Browser application deployment choices.
#[derive(Deserialize)]
pub struct DeploymentPreviewForm {
    _csrf: String,
    application: String,
    domain: String,
    php_version: String,
    locale: String,
}

/// Render a complete application deployment plan for confirmation.
pub async fn preview_deployment(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Form(form): Form<DeploymentPreviewForm>,
) -> Response {
    if !state.csrf.verify(session.id(), &form._csrf) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let preview = match state
        .software_center
        .preview_deployment(
            user.id(),
            user.role(),
            openpanel_app::software_center::ApplicationDeploymentInput {
                application: form.application,
                domain: form.domain,
                php_version: form.php_version,
                locale: form.locale,
                enable_dns: false,
                enable_tls: false,
                enable_backups: false,
            },
        )
        .await
    {
        Ok(value) => value,
        Err(_) => return StatusCode::UNPROCESSABLE_ENTITY.into_response(),
    };
    let csrf = state.csrf.token_for(session.id());
    let content = html! {
        h1 { "Review application deployment" }
        p { "Affected services: " (preview.affected_services.join(", ")) }
        code { (preview.plan.digest()) }
        form method="post" action=(format!("/software/applications/plans/{}/execute", preview.plan.digest())) {
            input type="hidden" name="_csrf" value=(csrf);
            input type="hidden" name="confirmation_token" value=(preview.confirmation_token);
            button { "Confirm deployment" }
        }
    };
    state
        .render_shell(&user, &csrf, "/software", content)
        .await
        .into_response()
}

/// Execute one application deployment and render its one-time credentials.
pub async fn execute_deployment(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Path(digest): Path<String>,
    Form(form): Form<ExecuteForm>,
) -> Response {
    if !state.csrf.verify(session.id(), &form._csrf) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let deployed = match state
        .software_center
        .execute_deployment(user.id(), user.role(), &digest, &form.confirmation_token)
        .await
    {
        Ok(value) => value,
        Err(_) => return StatusCode::UNPROCESSABLE_ENTITY.into_response(),
    };
    let csrf = state.csrf.token_for(session.id());
    let content = html! {
        h1 { "Application deployed" }
        p { "Save these administrator credentials now. They will not be shown again." }
        dl {
            dt { "Username" } dd { (deployed.admin_username) }
            dt { "Password" } dd { (deployed.admin_password) }
        }
        a href="/software" { "Return to Software Center" }
    };
    state
        .render_shell(&user, &csrf, "/software", content)
        .await
        .into_response()
}

/// CSRF-only form used to request a fresh preview.
#[derive(Deserialize)]
pub struct PreviewForm {
    _csrf: String,
}

/// Render a digest-bound installation confirmation.
pub async fn preview(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Path(id): Path<String>,
    Form(form): Form<PreviewForm>,
) -> Response {
    if !state.csrf.verify(session.id(), &form._csrf) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let preview = match state
        .software_center
        .preview_install(user.id(), user.role(), &id)
        .await
    {
        Ok(value) => value,
        Err(_) => return StatusCode::UNPROCESSABLE_ENTITY.into_response(),
    };
    let csrf = state.csrf.token_for(session.id());
    let content = html! {
        h1 { "Review software transaction" }
        p { "Affected services: " (preview.affected_services.join(", ")) }
        code { (preview.plan.digest()) }
        form method="post" action=(format!("/software/plans/{}/execute", preview.plan.digest())) {
            input type="hidden" name="_csrf" value=(csrf);
            input type="hidden" name="confirmation_token" value=(preview.confirmation_token);
            button { "Confirm installation" }
        }
    };
    state
        .render_shell(&user, &csrf, "/software", content)
        .await
        .into_response()
}

/// Confirmed execution form.
#[derive(Deserialize)]
pub struct ExecuteForm {
    _csrf: String,
    confirmation_token: String,
}

/// Execute exactly the reviewed plan.
pub async fn execute(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Path(digest): Path<String>,
    Form(form): Form<ExecuteForm>,
) -> Response {
    if !state.csrf.verify(session.id(), &form._csrf) {
        return StatusCode::FORBIDDEN.into_response();
    }
    match state
        .software_center
        .execute(user.id(), user.role(), &digest, &form.confirmation_token)
        .await
    {
        Ok(_) => StatusCode::OK.into_response(),
        Err(_) => StatusCode::UNPROCESSABLE_ENTITY.into_response(),
    }
}
