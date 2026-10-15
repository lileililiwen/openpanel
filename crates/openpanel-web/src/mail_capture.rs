//! Staging mail-capture inbox HTML surface.
//!
//! Lists captured messages for a site's staging slot, shows a single
//! decrypted message, and purges the inbox. Bodies are rendered only
//! here, never in lists, audit, or logs.

use axum::{
    Form,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
};
use maud::{Markup, html};
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    layout::csrf_field,
    router::{WebState, WebUser},
};

/// GET /mail/capture/{site_id} — body-free inbox list.
pub async fn list(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Path(site_id): Path<Uuid>,
) -> Response {
    let csrf = state.csrf.token_for(session.id());
    match state.mail_capture.list(&user, site_id).await {
        Ok(messages) => {
            let body = content(site_id, &messages, &csrf, None);
            state
                .render_shell(&user, &csrf, "/mail", body)
                .await
                .into_response()
        }
        Err(error) => (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
    }
}

/// GET /mail/capture/{site_id}/{message_id} — one decrypted message.
pub async fn detail(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Path((site_id, message_id)): Path<(Uuid, Uuid)>,
) -> Response {
    let csrf = state.csrf.token_for(session.id());
    match state.mail_capture.read(&user, site_id, message_id).await {
        Ok(message) => {
            let body = detail_content(site_id, &message);
            state
                .render_shell(&user, &csrf, "/mail", body)
                .await
                .into_response()
        }
        Err(error) => (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
    }
}

#[derive(Deserialize)]
/// CSRF-protected purge form.
pub struct PurgeForm {
    _csrf: String,
}

/// POST /mail/capture/{site_id}/purge — drop every captured message.
pub async fn purge(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Path(site_id): Path<Uuid>,
    Form(form): Form<PurgeForm>,
) -> Response {
    if !state.csrf.verify(session.id(), &form._csrf) {
        return StatusCode::FORBIDDEN.into_response();
    }
    match state.mail_capture.purge(&user, site_id).await {
        Ok(removed) => {
            let csrf = state.csrf.token_for(session.id());
            let messages = state
                .mail_capture
                .list(&user, site_id)
                .await
                .unwrap_or_default();
            let banner = format!("Purged {removed} captured message(s).");
            let body = content(site_id, &messages, &csrf, Some(&banner));
            state
                .render_shell(&user, &csrf, "/mail", body)
                .await
                .into_response()
        }
        Err(error) => (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
    }
}

#[derive(Deserialize)]
/// Toggle form posted from the staging page.
pub struct ToggleForm {
    _csrf: String,
    enabled: Option<String>,
}

/// POST /sites/{site_id}/staging/mail-capture — flip the capture toggle.
pub async fn toggle(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Path(site_id): Path<Uuid>,
    Form(form): Form<ToggleForm>,
) -> Response {
    if !state.csrf.verify(session.id(), &form._csrf) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let enabled = form.enabled.is_some();
    match state
        .staging
        .set_mail_capture(&user, site_id, enabled)
        .await
    {
        Ok(_) => Redirect::to(&format!("/sites/{site_id}/staging")).into_response(),
        Err(error) => (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
    }
}

fn content(
    site_id: Uuid,
    messages: &[openpanel_domain::CapturedMessageSummary],
    csrf: &str,
    banner: Option<&str>,
) -> Markup {
    html! {
        h1 { "Staging mail capture" }
        @if let Some(message)=banner { p class="notice" { (message) } }
        p {
          "Captured mail for this site's staging slot while capture is enabled on the "
          a href=(format!("/sites/{site_id}/staging")) { "staging page" }
          "."
        }
        @if !messages.is_empty() {
            form method="post" action=(format!("/mail/capture/{site_id}/purge")) class="form form-inline" {
                (csrf_field(csrf))
                button class="btn danger" type="submit" { "Purge inbox" }
            }
        }
        @if messages.is_empty() {
            (crate::ui_states::EmptyState::new(
                "No captured messages",
                "Form submissions from the staging site land here instead of the real recipient.",
            ).with_cta(&format!("/sites/{site_id}/staging"), "Back to staging").render())
        } @else {
            table class="table" {
                thead { tr {
                    th { "From" }
                    th { "To" }
                    th { "Subject" }
                    th { "Captured" }
                    th { "Open" }
                } }
                tbody { @for m in messages { tr {
                    td { (&m.mailbox) }
                    td { (&m.recipient) }
                    td { (&m.subject) }
                    td { (&m.captured_at.to_rfc3339()) }
                    td { a class="btn btn-ghost" href=(format!("/mail/capture/{site_id}/{}", m.id)) { "View" } }
                } } }
            }
        }
    }
}

fn detail_content(site_id: Uuid, message: &openpanel_app::ReadCapturedMessageView) -> Markup {
    html! {
        h1 { "Captured message" }
        p { a href=(format!("/mail/capture/{site_id}")) { "Back to inbox" } }
        div class="card" {
            p { strong { "From: " } (message.mailbox) }
            p { strong { "To: " } (message.recipient) }
            p { strong { "Subject: " } (message.subject) }
            p { strong { "Captured: " } (message.captured_at.to_rfc3339()) }
            pre { (&message.body) }
        }
    }
}
