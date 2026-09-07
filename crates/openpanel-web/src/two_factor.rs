//! Two-factor authentication settings page.

use axum::{
    Form, Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
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

/// TOTP enrollment verification form.
#[derive(Deserialize)]
pub struct VerifyTotpForm {
    /// CSRF token.
    pub _csrf: String,
    /// Pending enrollment id.
    pub enrollment_id: Uuid,
    /// Six-digit code from the authenticator.
    pub code: String,
}

fn valid_csrf_header(headers: &HeaderMap, state: &WebState, session_id: Uuid) -> bool {
    headers
        .get("x-csrf-token")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|token| state.csrf.verify(session_id, token))
}

/// `POST /settings/security/webauthn/register/begin` — begin a
/// browser-bound passkey ceremony with CSRF protection.
pub async fn begin_webauthn(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    headers: HeaderMap,
) -> Response {
    if !valid_csrf_header(&headers, &state, session.id()) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let now = chrono::Utc::now();
    let result = state
        .two_factor
        .begin_webauthn_register(user.id(), user.username().as_str());
    let (challenge_id, public_key, state_json) = match result {
        Ok(value) => value,
        Err(error) => {
            return (StatusCode::UNPROCESSABLE_ENTITY, error.to_string()).into_response();
        }
    };
    if let Err(error) = state
        .two_factor
        .persist_webauthn_register(challenge_id, user.id(), &state_json, now)
        .await
    {
        return (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()).into_response();
    }
    Json(serde_json::json!({
        "challenge_id": challenge_id,
        "public_key": public_key,
    }))
    .into_response()
}

/// Browser payload completing a passkey registration ceremony.
#[derive(Debug, Deserialize)]
pub struct FinishWebAuthnForm {
    /// Panel-side ceremony id.
    challenge_id: Uuid,
    /// Serialized `PublicKeyCredential` returned by the browser.
    credential: serde_json::Value,
}

/// `POST /settings/security/webauthn/register/finish`.
pub async fn finish_webauthn(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    headers: HeaderMap,
    Json(form): Json<FinishWebAuthnForm>,
) -> Response {
    if !valid_csrf_header(&headers, &state, session.id()) {
        return StatusCode::FORBIDDEN.into_response();
    }
    match state
        .two_factor
        .finish_webauthn_register(
            user.username().as_str(),
            form.challenge_id,
            user.id(),
            &form.credential,
            chrono::Utc::now(),
        )
        .await
    {
        Ok(_) => Json(serde_json::json!({"ok": true})).into_response(),
        Err(error) => (StatusCode::UNPROCESSABLE_ENTITY, error.to_string()).into_response(),
    }
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
            user.username().as_str(),
            now,
        )
        .await;
    match result {
        Ok(enrollment) => {
            let csrf = state.csrf.token_for(session.id());
            let content = enrollment_content(
                enrollment.enrollment_id,
                &enrollment.secret.to_base32(),
                &enrollment.provisioning_uri,
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

/// `POST /settings/security/totp/verify` — activate a pending TOTP enrollment.
pub async fn verify_totp(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Form(form): Form<VerifyTotpForm>,
) -> Response {
    if !state.csrf.verify(session.id(), &form._csrf) {
        return StatusCode::FORBIDDEN.into_response();
    }
    match state
        .two_factor
        .verify_totp_enrollment(
            user.username().as_str(),
            user.id(),
            form.enrollment_id,
            &form.code,
            chrono::Utc::now(),
        )
        .await
    {
        Ok(verified) => {
            let csrf = state.csrf.token_for(session.id());
            let content = recovery_content(&verified.recovery_codes, &csrf);
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
                            table class="table" {
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
                                                    form method="post" action={"/settings/security/factors/" (factor.id()) "/revoke"} class="form form-inline" {
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
                        form method="post" action="/settings/security/totp/enroll" class="form form-inline" {
                            input type="hidden" name="_csrf" value=(csrf);
                            button type="submit" class="button" { "Enroll TOTP" }
                        }
                        button type="button" class="button" id="enroll-webauthn" { "Enroll passkey" }
                        p id="webauthn-status" role="status" {}
                    }
                    section class="card" {
                        h2 { "Recovery codes" }
                        p { "Remaining: " (recovery_remaining) }
                        form method="post" action="/settings/security/recovery/regenerate" class="form form-inline" {
                            input type="hidden" name="_csrf" value=(csrf);
                            button type="submit" class="button" { "Regenerate recovery codes" }
                        }
                    }
                    script {
                        (maud::PreEscaped(format!(r#"
const csrf = {csrf:?};
const status = document.getElementById('webauthn-status');
const fromB64 = value => Uint8Array.from(atob(value.replace(/-/g, '+').replace(/_/g, '/') + '='.repeat((4 - value.length % 4) % 4)), c => c.charCodeAt(0));
const toB64 = value => btoa(String.fromCharCode(...new Uint8Array(value))).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
document.getElementById('enroll-webauthn').addEventListener('click', async () => {{
  try {{
    const begin = await fetch('/settings/security/webauthn/register/begin', {{method: 'POST', headers: {{'x-csrf-token': csrf}}}});
    if (!begin.ok) throw new Error(await begin.text());
    const ceremony = await begin.json();
    const options = ceremony.public_key.publicKey || ceremony.public_key;
    options.challenge = fromB64(options.challenge);
    options.user.id = fromB64(options.user.id);
    if (options.excludeCredentials) options.excludeCredentials.forEach(c => c.id = fromB64(c.id));
    const credential = await navigator.credentials.create({{publicKey: options}});
    const body = {{
      challenge_id: ceremony.challenge_id,
      credential: {{
        id: credential.id,
        rawId: toB64(credential.rawId),
        type: credential.type,
        response: {{
          clientDataJSON: toB64(credential.response.clientDataJSON),
          attestationObject: toB64(credential.response.attestationObject),
          transports: credential.response.getTransports ? credential.response.getTransports() : []
        }}
      }}
    }};
    const finish = await fetch('/settings/security/webauthn/register/finish', {{method: 'POST', headers: {{'content-type': 'application/json', 'x-csrf-token': csrf}}, body: JSON.stringify(body)}});
    if (!finish.ok) throw new Error(await finish.text());
    window.location.reload();
  }} catch (error) {{ status.textContent = `Passkey enrollment failed: ${{error.message}}`; }}
}});
"#)))
                    }
                }
            }
        }
    }
}

fn enrollment_content(
    enrollment_id: Uuid,
    secret_base32: &str,
    provisioning_uri: &str,
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
                    h1 { "Verify TOTP enrollment" }
                    div class="banner banner--ok" {
                        "The secret below is shown only once. Add it to your authenticator, then enter the current six-digit code to activate it."
                    }
                    section class="card" {
                        h2 { "TOTP secret" }
                        p class="config__path" { code { (secret_base32) } }
                        p { "Provisioning URI: " code { (provisioning_uri) } }
                    }
                    form method="post" action="/settings/security/totp/verify" class="form" {
                        input type="hidden" name="_csrf" value=(csrf);
                        input type="hidden" name="enrollment_id" value=(enrollment_id);
                        label { "Authenticator code" input id="totp-code" name="code" inputmode="numeric" autocomplete="one-time-code" required; }
                        button type="submit" class="button" { "Verify and activate" }
                        }
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
