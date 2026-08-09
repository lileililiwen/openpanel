//! Hosted mail browser pages.

use axum::{
    Form,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use maud::html;
use serde::Deserialize;

use crate::router::{WebState, WebUser};

/// Render readiness and secret-free domains.
pub async fn page(State(state): State<WebState>, WebUser(user, session): WebUser) -> Response {
    let csrf = state.csrf.token_for(session.id());
    let domains = state
        .mail
        .domains(user.id(), user.role())
        .await
        .unwrap_or_default();
    let readiness = state.mail.readiness().await.ok();
    let content = html! {
        h1 { "Mail hosting" }
        @if let Some(readiness) = readiness {
            p { "ready=" (readiness.ready) }
        }
        ul {
            @for domain in domains {
                li { (domain.name.as_str()) }
            }
        }
        form method="post" action="/mail/domains" {
            input type="hidden" name="_csrf" value=(csrf);
            input name="name";
            button { "Add mail domain" }
        }
    };
    state
        .render_shell(&user, &csrf, "/mail", content)
        .await
        .into_response()
}

/// Mail domain form.
#[derive(Default, Deserialize)]
#[serde(default)]
pub struct DomainForm {
    _csrf: String,
    name: String,
}

/// Create a disabled mail domain with CSRF enforcement.
pub async fn create_domain(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Form(form): Form<DomainForm>,
) -> Response {
    if !state.csrf.verify(session.id(), &form._csrf) {
        return StatusCode::FORBIDDEN.into_response();
    }
    match state
        .mail
        .create_domain(user.id(), user.role(), &form.name)
        .await
    {
        Ok(_) => StatusCode::CREATED.into_response(),
        Err(error) => (StatusCode::UNPROCESSABLE_ENTITY, error.to_string()).into_response(),
    }
}
