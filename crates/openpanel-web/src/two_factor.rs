//! Two-factor authentication settings page.

use axum::{
    Form,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
};
use maud::{DOCTYPE, Markup, html};
use openpanel_domain::identity::Factor;
use serde::Deserialize;
use uuid::Uuid;

use crate::router::{WebState, WebUser};

/// `GET /settings/security` — render the 2FA management page.
pub async fn page(State(state): State<WebState>, WebUser(user, session): WebUser) -> Response {
    let csrf = state.csrf.token_for(session.id());
    let factors = match state.two_factor.list_factors(user.id()).await {
        Ok(value) => value,
        Err(error) => {
            return error_page_response(&error.to_string(), &state, &user, &csrf).await;
        }
    };
    let remaining = state
        .two_factor
        .count_recovery_codes(user.id())
        .await
        .unwrap_or(0);
    let content = security_content(&factors, remaining, &csrf);
    state
        .render_shell(&user, &csrf, "/settings/security", content)
        .await
        .into_response()
}

/// Minimal form body carrying only the CSRF token.
#[derive(Deserialize)]
pub struct EmptyCsrf {
    /// CSRF token echoed from the form.
    pub _csrf: String,
}

/// `POST /settings/security/totp/enroll` — enroll TOTP for the current user.
pub async fn enroll_totp(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Form(form): Form<EmptyCsrf>,
) -> Response {
    if !state.csrf.verify(session.id(), &form._csrf) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let now = chrono::Utc::now();
    let result = state
        .two_factor
        .enroll_totp(
            user.id(),
            user.username().as_str(),
            "OpenPanel",
            user.username().as_str(),
            now,
        )
        .await;
    match result {
        Ok(enrollment) => {
            let csrf = state.csrf.token_for(session.id());
            let content = enrollment_content(
                &enrollment.secret.to_base32(),
                &enrollment.provisioning_uri,
                &enrollment.recovery_codes,
                &csrf,
            );
            state
                .render_shell(&user, &csrf, "/settings/security", content)
                .await
                .into_response()
        }
        Err(error) => {
            let csrf = state.csrf.token_for(session.id());
            error_page_response(&error.to_string(), &state, &user, &csrf).await
        }
    }
}

/// Form body for revoking a factor.
#[derive(Deserialize)]
pub struct RevokeFactorForm {
    /// CSRF token echoed from the form.
    pub _csrf: String,
    /// Factor id (unused — the URL path carries it) kept for form parity.
    pub factor_id: String,
}

/// `POST /settings/security/factors/{id}/revoke`.
pub async fn revoke_factor(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Path(id): Path<String>,
    Form(form): Form<RevokeFactorForm>,
) -> Response {
    if !state.csrf.verify(session.id(), &form._csrf) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let factor_id = match Uuid::parse_str(&id) {
        Ok(value) => value,
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };
    let now = chrono::Utc::now();
    let result = state
        .two_factor
        .revoke_factor(user.username().as_str(), user.id(), factor_id, now)
        .await;
    if let Err(error) = result {
        let csrf = state.csrf.token_for(session.id());
        return error_page_response(&error.to_string(), &state, &user, &csrf).await;
    }
    Redirect::to("/settings/security").into_response()
}

/// `POST /settings/security/recovery/regenerate`.
pub async fn regenerate_recovery(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Form(form): Form<EmptyCsrf>,
) -> Response {
    if !state.csrf.verify(session.id(), &form._csrf) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let now = chrono::Utc::now();
    let result = state
        .two_factor
        .regenerate_recovery_codes(user.username().as_str(), user.id(), now)
        .await;
    match result {
        Ok(codes) => {
            let csrf = state.csrf.token_for(session.id());
            let content = recovery_content(&codes, &csrf);
            state
                .render_shell(&user, &csrf, "/settings/security", content)
                .await
                .into_response()
        }
        Err(error) => {
            let csrf = state.csrf.token_for(session.id());
            error_page_response(&error.to_string(), &state, &user, &csrf).await
        }
    }
}

fn security_content(factors: &[Factor], recovery_remaining: u8, csrf: &str) -> Markup {
    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                title { "OpenPanel — Security" }
                link rel="stylesheet" href="/assets/app.css";
            }
            body {
                main class="settings" {
                    h1 { "Two-factor authentication" }
                    section class="card" {
                        h2 { "Enrolled factors" }
                        @if factors.is_empty() {
                            p { "No factors enrolled. Add a TOTP factor below." }
                        } @else {
                            table {
                                thead { tr { th { "Kind" } th { "Enrolled" } th { "Last used" } th { "Status" } th { "" } } }
                                tbody {
                                    @for factor in factors {
                                        tr {
                                            td { (factor.kind().to_string()) }
                                            td { (factor.created_at().to_rfc3339()) }
                                            td { @if let Some(ts) = factor.last_used_at() { (ts.to_rfc3339()) } @else { "—" } }
                                            td { @if factor.revoked_at().is_some() { "revoked" } @else { "active" } }
                                            td {
                                                @if factor.revoked_at().is_none() {
                                                    form method="post" action={"/settings/security/factors/" (factor.id()) "/revoke"} {
                                                        input type="hidden" name="_csrf" value=(csrf);
                                                        button type="submit" { "Revoke" }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        form method="post" action="/settings/security/totp/enroll" {
                            input type="hidden" name="_csrf" value=(csrf);
                            button type="submit" class="button" { "Enroll TOTP" }
                        }
                    }
                    section class="card" {
                        h2 { "Recovery codes" }
                        p { "Remaining: " (recovery_remaining) }
                        form method="post" action="/settings/security/recovery/regenerate" {
                            input type="hidden" name="_csrf" value=(csrf);
                            button type="submit" class="button" { "Regenerate recovery codes" }
                        }
                    }
                }
            }
        }
    }
}

fn enrollment_content(
    secret_base32: &str,
    provisioning_uri: &str,
    recovery_codes: &[String],
    csrf: &str,
) -> Markup {
    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                title { "OpenPanel — TOTP enrolled" }
                link rel="stylesheet" href="/assets/app.css";
            }
            body {
                main class="settings" {
                    h1 { "TOTP enrolled — save these now" }
                    div class="banner banner--ok" {
                        "The secret and recovery codes below are shown only once. Save them in a password manager or write them down."
                    }
                    section class="card" {
                        h2 { "TOTP secret" }
                        p class="config__path" { code { (secret_base32) } }
                        p { "Provisioning URI: " code { (provisioning_uri) } }
                    }
                    section class="card" {
                        h2 { "Recovery codes" }
                        ul {
                            @for code in recovery_codes {
                                li { code { (code) } }
                            }
                        }
                    }
                    a class="button" href="/settings/security" { "Done" }
                    input type="hidden" name="_csrf" value=(csrf);
                }
            }
        }
    }
}

fn recovery_content(codes: &[String], csrf: &str) -> Markup {
    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                title { "OpenPanel — Recovery codes" }
                link rel="stylesheet" href="/assets/app.css";
            }
            body {
                main class="settings" {
                    h1 { "New recovery codes" }
                    div class="banner banner--ok" {
                        "These single-use codes replace the previous set. Save them now — the previous codes no longer work."
                    }
                    section class="card" {
                        h2 { "Recovery codes" }
                        ul {
                            @for code in codes {
                                li { code { (code) } }
                            }
                        }
                    }
                    a class="button" href="/settings/security" { "Done" }
                    input type="hidden" name="_csrf" value=(csrf);
                }
            }
        }
    }
}

fn error_page_markup(message: &str) -> Markup {
    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                title { "OpenPanel — Security error" }
                link rel="stylesheet" href="/assets/app.css";
            }
            body {
                main class="settings" {
                    h1 { "Security" }
                    div class="banner banner--error" { (message) }
                    a class="button" href="/settings/security" { "Back" }
                }
            }
        }
    }
}

async fn error_page_response(
    message: &str,
    state: &WebState,
    user: &openpanel_domain::User,
    csrf: &str,
) -> Response {
    let content = error_page_markup(message);
    state
        .render_shell(user, csrf, "/settings/security", content)
        .await
        .into_response()
}
