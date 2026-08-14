//! Owner-only container list, create form, lifecycle, and logs.

use axum::{
    Form,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use maud::html;
use openpanel_domain::{Role, docker::ContainerSpec};
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    layout::csrf_field,
    router::{WebState, WebUser},
};

/// Render managed containers and the strict JSON create form.
pub async fn page(State(state): State<WebState>, WebUser(user, session): WebUser) -> Response {
    if user.role() != Role::Owner {
        return StatusCode::FORBIDDEN.into_response();
    }
    let csrf = state.csrf.token_for(session.id());
    render(&state, &user, &csrf, None).await
}

/// Browser create form.
#[derive(Deserialize)]
pub struct CreateForm {
    _csrf: String,
    spec_json: String,
}

/// Validate CSRF and create one container.
pub async fn create(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Form(form): Form<CreateForm>,
) -> Response {
    if user.role() != Role::Owner || !state.csrf.verify(session.id(), &form._csrf) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let spec = match serde_json::from_str::<ContainerSpec>(&form.spec_json) {
        Ok(value) => value,
        Err(error) => return (StatusCode::UNPROCESSABLE_ENTITY, error.to_string()).into_response(),
    };
    match state.docker.create(&user, spec).await {
        Ok(_) => render(&state, &user, &form._csrf, Some("Container created")).await,
        Err(error) => (StatusCode::UNPROCESSABLE_ENTITY, error.to_string()).into_response(),
    }
}

/// Browser lifecycle form.
#[derive(Deserialize)]
pub struct ActionForm {
    _csrf: String,
}

/// Start, stop, restart, or remove one container.
pub async fn action(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Path((id, action)): Path<(Uuid, String)>,
    Form(form): Form<ActionForm>,
) -> Response {
    if user.role() != Role::Owner || !state.csrf.verify(session.id(), &form._csrf) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let result = if action == "remove" {
        state.docker.remove(&user, id, false).await
    } else {
        state.docker.action(&user, id, &action).await.map(|_| ())
    };
    match result {
        Ok(()) => {
            render(
                &state,
                &user,
                &form._csrf,
                Some("Container lifecycle updated"),
            )
            .await
        }
        Err(error) => (StatusCode::UNPROCESSABLE_ENTITY, error.to_string()).into_response(),
    }
}

/// Render a bounded log tail.
pub async fn logs(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Path(id): Path<Uuid>,
) -> Response {
    if user.role() != Role::Owner {
        return StatusCode::FORBIDDEN.into_response();
    }
    let csrf = state.csrf.token_for(session.id());
    match state.docker.logs(&user, id, 100).await {
        Ok(lines) => state
            .render_shell(
                &user,
                &csrf,
                "/docker",
                html! { h1 { "Container logs" } pre { @for line in lines { (line) } } },
            )
            .await
            .into_response(),
        Err(error) => (StatusCode::UNPROCESSABLE_ENTITY, error.to_string()).into_response(),
    }
}

async fn render(
    state: &WebState,
    user: &openpanel_domain::User,
    csrf: &str,
    notice: Option<&str>,
) -> Response {
    let rows = state.docker.list(user).await.unwrap_or_default();
    let content = html! {
        h1 { "Containers" }
        p class="banner banner--warning" { "Forbidden capabilities, root users, unsafe mounts, and untrusted images are rejected before the daemon is contacted." }
        @if let Some(notice) = notice { p class="banner banner--ok" { (notice) } }
        form method="post" action="/docker" class="form" {
            (csrf_field(csrf))
            label { "Container specification (strict JSON)" textarea name="spec_json" rows="12" { "{}" } }
            button type="submit" { "Create container" }
        }
        h2 { "Managed containers" }
        @if rows.is_empty() { p { "No managed containers." } } @else {
            ul { @for row in rows { li {
                (row.spec.name) " — " (row.status) " "
                a href=(format!("/docker/{}/logs", row.spec.id)) { "Logs" }
                @for action in ["start", "stop", "restart", "remove"] {
                    form method="post" action=(format!("/docker/{}/{action}", row.spec.id)) class="form form-inline" {
                        (csrf_field(csrf)) button type="submit" { (action) }
                    }
                }
            } } }
        }
    };
    state
        .render_shell(user, csrf, "/docker", content)
        .await
        .into_response()
}
