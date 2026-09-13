//! Webmail client web pages: server-rendered maud HTML, session
//! gated via a short-lived `?ws=<token>` query param. Mutations
//! (compose) are CSRF-protected through the session middleware and
//! the JSON API. The plaintext mailbox password never appears in
//! any rendered output.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use maud::{Markup, html};
use serde::Deserialize;

use crate::router::{WebState, WebUser};

/// Query params accepted on `/webmail` entry.
#[derive(Debug, Deserialize)]
pub struct WsQuery {
    /// Short-lived webmail session token.
    ws: Option<String>,
}

/// Compose form payload submitted to `/webmail/compose`.
#[derive(Debug, Deserialize)]
pub struct ComposeBody {
    /// Short-lived webmail session token.
    ws: String,
    /// Recipient address.
    to: String,
    /// Message subject.
    subject: String,
    /// Message body (HTML).
    body: String,
}

/// GET /webmail — mint a session on first entry, then redirect to
/// the folder list.
///
/// `mailbox-surfaces`: the session is bound to the authenticated
/// user's authorized mailbox (own address first, else first visible
/// mailbox). There is no fixed demo mailbox; callers without an
/// authorized mailbox receive `403` with a safe error state.
pub async fn index(
    State(state): State<WebState>,
    WebUser(user, panel_session): WebUser,
    Query(query): Query<WsQuery>,
) -> Response {
    let ws = match query.ws {
        Some(ws) => ws,
        None => {
            let mailbox = match state
                .mail
                .default_mailbox_for_user(user.id(), user.role(), user.email().as_str())
                .await
            {
                Ok(mailbox) => mailbox,
                Err(_) => {
                    return (StatusCode::FORBIDDEN, "no authorized mailbox").into_response();
                }
            };
            let session = match state
                .webmail
                .mint_session(&user.id().to_string(), mailbox.address.as_str())
                .await
            {
                Ok(s) => s,
                Err(_) => {
                    return (StatusCode::INTERNAL_SERVER_ERROR, "session mint failed")
                        .into_response();
                }
            };
            session.token().to_string()
        }
    };
    let session = match state.webmail.validate_session(&ws).await {
        Ok(s) => s,
        Err(_) => {
            return (
                StatusCode::UNAUTHORIZED,
                "webmail session expired; <a href=\"/webmail\">re-authenticate</a>",
            )
                .into_response();
        }
    };
    if state
        .mail
        .resolve_authorized_mailbox(user.id(), user.role(), session.mailbox())
        .await
        .is_err()
    {
        return (StatusCode::FORBIDDEN, "no authorized mailbox").into_response();
    }
    let folders = match state.webmail.folders(&session).await {
        Ok(f) => f,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "mailbox unavailable; try again later",
            )
                .into_response();
        }
    };
    let body: Markup = html! {
        section class="card" {
            h2 { "Webmail" }
            p { "Session: " code { (session.mailbox()) } " (15-minute token)" }
            ul {
                @for folder in &folders {
                    li { a href={ "/webmail/folder/" (folder) "?ws=" (ws) } { (folder) } }
                }
            }
            a class="button" href={ "/webmail/compose?ws=" (ws) } { "Compose" }
        }
    };
    let csrf = state.csrf.token_for(panel_session.id());
    state
        .render_shell(&user, &csrf, "/webmail", body)
        .await
        .into_response()
}

/// GET /webmail/folder/{name} — message list with cursor-style cap.
pub async fn folder(
    State(state): State<WebState>,
    WebUser(user, panel_session): WebUser,
    Query(query): Query<WsQuery>,
    Path(name): Path<String>,
) -> Response {
    let Some(ws) = query.ws else {
        return (StatusCode::UNAUTHORIZED, "missing webmail session").into_response();
    };
    let session = match state.webmail.validate_session(&ws).await {
        Ok(s) => s,
        Err(_) => return (StatusCode::UNAUTHORIZED, "expired session").into_response(),
    };
    if state
        .mail
        .resolve_authorized_mailbox(user.id(), user.role(), session.mailbox())
        .await
        .is_err()
    {
        return (StatusCode::FORBIDDEN, "no authorized mailbox").into_response();
    }
    let messages = match state.webmail.messages(&session, &name, 50).await {
        Ok(m) => m,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "mailbox unavailable; try again later",
            )
                .into_response();
        }
    };
    let body: Markup = html! {
        section class="card" {
            h2 { "Folder: " (name) }
            ul {
                @for m in &messages {
                    li {
                        a href={ "/webmail/message/" (m.uid) "?ws=" (ws) } {
                            strong { (m.subject) } " — " (m.from)
                        }
                    }
                }
            }
        }
    };
    let csrf = state.csrf.token_for(panel_session.id());
    state
        .render_shell(&user, &csrf, "/webmail", body)
        .await
        .into_response()
}

/// GET /webmail/message/{id} — redacted message view.
pub async fn message(
    State(state): State<WebState>,
    WebUser(user, panel_session): WebUser,
    Query(query): Query<WsQuery>,
    Path(uid): Path<u64>,
) -> Response {
    let Some(ws) = query.ws else {
        return (StatusCode::UNAUTHORIZED, "missing webmail session").into_response();
    };
    let session = match state.webmail.validate_session(&ws).await {
        Ok(s) => s,
        Err(_) => return (StatusCode::UNAUTHORIZED, "expired session").into_response(),
    };
    if state
        .mail
        .resolve_authorized_mailbox(user.id(), user.role(), session.mailbox())
        .await
        .is_err()
    {
        return (StatusCode::FORBIDDEN, "no authorized mailbox").into_response();
    }
    let msg = match state.webmail.message(&session, uid).await {
        Ok(m) => m,
        Err(_) => return (StatusCode::NOT_FOUND, "message not found").into_response(),
    };
    let body: Markup = html! {
        section class="card" {
            h2 { (msg.subject) }
            p { "From: " (msg.from) " | To: " (msg.to) " | " (msg.date.format("%Y-%m-%d %H:%M")) }
            div class="message-body" { (maud::PreEscaped(msg.html)) }
            a class="button" href={ "/webmail/message/" (uid) "/reply?ws=" (ws) } { "Reply" }
            a class="button" href={ "/webmail/message/" (uid) "/forward?ws=" (ws) } { "Forward" }
        }
    };
    let csrf = state.csrf.token_for(panel_session.id());
    state
        .render_shell(&user, &csrf, "/webmail", body)
        .await
        .into_response()
}

/// GET /webmail/compose — compose form (POST to /webmail/compose).
pub async fn compose_form(
    State(state): State<WebState>,
    WebUser(user, panel_session): WebUser,
    Query(query): Query<WsQuery>,
) -> Response {
    let Some(ws) = query.ws else {
        return (StatusCode::UNAUTHORIZED, "missing webmail session").into_response();
    };
    let body: Markup = html! {
        section class="card" {
            h2 { "Compose" }
            form method="post" action="/webmail/compose" class="form form-grid" {
                input type="hidden" name="ws" value={(ws)};
                label { "To" input type="email" name="to" required; }
                label { "Subject" input type="text" name="subject" required; }
                label { "Body" textarea name="body" rows="10" required; }
                div class="form-actions" {
                    button type="submit" { "Send" }
                }
            }
        }
    };
    let csrf = state.csrf.token_for(panel_session.id());
    state
        .render_shell(&user, &csrf, "/webmail", body)
        .await
        .into_response()
}

/// POST /webmail/compose — CSRF-protected compose submission.
pub async fn compose_send(
    State(state): State<WebState>,
    WebUser(user, _): WebUser,
    axum::Form(body): axum::Form<ComposeBody>,
) -> Response {
    let session = match state.webmail.validate_session(&body.ws).await {
        Ok(s) => s,
        Err(_) => return (StatusCode::UNAUTHORIZED, "expired session").into_response(),
    };
    if state
        .mail
        .resolve_authorized_mailbox(user.id(), user.role(), session.mailbox())
        .await
        .is_err()
    {
        return (StatusCode::FORBIDDEN, "no authorized mailbox").into_response();
    }
    let Some(quota) = openpanel_domain::mail::MailboxQuota::new(1_000_000, 1, 1_000_000_000).ok()
    else {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            "mailbox unavailable; try again later",
        )
            .into_response();
    };
    let domain = session
        .mailbox()
        .rsplit_once('@')
        .map_or("example.invalid", |(_, domain)| domain);
    let policy = openpanel_domain::mail::DomainSendingPolicy::default_for(domain);
    match state
        .webmail
        .send(
            &session,
            &body.to,
            &body.subject,
            &body.body,
            &body.body,
            quota,
            0,
            policy,
        )
        .await
    {
        Ok(()) => (
            StatusCode::SEE_OTHER,
            [("Location", format!("/webmail?ws={}", body.ws))],
        )
            .into_response(),
        Err(_) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            "send refused; check recipients and quota",
        )
            .into_response(),
    }
}

/// GET /webmail/message/{id}/reply — reply form.
pub async fn reply(
    State(state): State<WebState>,
    WebUser(user, panel_session): WebUser,
    Query(query): Query<WsQuery>,
    Path(uid): Path<u64>,
) -> Response {
    let Some(ws) = query.ws else {
        return (StatusCode::UNAUTHORIZED, "missing webmail session").into_response();
    };
    let body: Markup = html! {
        section class="card" {
            h2 { "Reply to message " (uid) }
            form method="post" action="/webmail/compose" class="form form-grid" {
                input type="hidden" name="ws" value={(ws)};
                label { "To" input type="email" name="to" required; }
                label { "Subject" input type="text" name="subject" value="Re: ..." required; }
                label { "Body" textarea name="body" rows="10" required; }
                div class="form-actions" {
                    button type="submit" { "Send reply" }
                }
            }
        }
    };
    let csrf = state.csrf.token_for(panel_session.id());
    state
        .render_shell(&user, &csrf, "/webmail", body)
        .await
        .into_response()
}

/// GET /webmail/message/{id}/forward — forward form.
pub async fn forward(
    State(state): State<WebState>,
    WebUser(user, panel_session): WebUser,
    Query(query): Query<WsQuery>,
    Path(uid): Path<u64>,
) -> Response {
    let Some(ws) = query.ws else {
        return (StatusCode::UNAUTHORIZED, "missing webmail session").into_response();
    };
    let body: Markup = html! {
        section class="card" {
            h2 { "Forward message " (uid) }
            form method="post" action="/webmail/compose" class="form form-grid" {
                input type="hidden" name="ws" value={(ws)};
                label { "To" input type="email" name="to" required; }
                label { "Subject" input type="text" name="subject" value="Fwd: ..." required; }
                label { "Body" textarea name="body" rows="10" required; }
                div class="form-actions" {
                    button type="submit" { "Send forward" }
                }
            }
        }
    };
    let csrf = state.csrf.token_for(panel_session.id());
    state
        .render_shell(&user, &csrf, "/webmail", body)
        .await
        .into_response()
}

/// GET /webmail/search — search form.
pub async fn search(
    State(state): State<WebState>,
    WebUser(user, panel_session): WebUser,
    Query(query): Query<WsQuery>,
) -> Response {
    let Some(ws) = query.ws else {
        return (StatusCode::UNAUTHORIZED, "missing webmail session").into_response();
    };
    let body: Markup = html! {
        section class="card" {
            h2 { "Search" }
            form method="get" action="/webmail/folder/INBOX" class="form" {
                input type="hidden" name="ws" value={(ws)};
                label { "Query" input type="text" name="q" placeholder="search subject / from" ; }
                div class="form-actions" {
                    button type="submit" { "Search" }
                }
            }
        }
    };
    let csrf = state.csrf.token_for(panel_session.id());
    state
        .render_shell(&user, &csrf, "/webmail", body)
        .await
        .into_response()
}
