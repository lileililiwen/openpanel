//! Login page and login/logout handlers.
//!
//! Login is the one unauthenticated web route (there is no session to bind a
//! CSRF token to yet), so it accepts a plain form POST. Successful login sets
//! the `openpanel_session` cookie; failures render the page with a 401.

use std::net::{IpAddr, SocketAddr};

use axum::{
    extract::{ConnectInfo, Form, State},
    http::{HeaderMap, HeaderValue, StatusCode, header::RETRY_AFTER},
    response::{IntoResponse, Redirect, Response},
};
use maud::{DOCTYPE, Markup, html};
use openpanel_api::middleware::session::SESSION_COOKIE;
use openpanel_app::identity::{FactorResponse, LoginOutcome};
use serde::Deserialize;

use crate::router::WebState;

/// Body of the login form.
#[derive(Debug, Deserialize)]
pub struct LoginForm {
    /// Username or email address.
    pub username_or_email: String,
    /// Plaintext password.
    pub password: String,
}

/// Body of the second-factor form presented after the password step.
#[derive(Debug, Deserialize)]
pub struct FactorForm {
    /// Challenge id from the prior `factor_required` response.
    pub challenge_id: String,
    /// Plaintext challenge token held by the browser.
    pub challenge_token: String,
    /// Either `totp` (default) or `recovery`.
    #[serde(default)]
    pub kind: String,
    /// The TOTP code or recovery code.
    pub code: String,
    /// When `true`, the panel issues a remember-device cookie so this
    /// browser can skip the factor step on the next login.
    #[serde(default)]
    pub remember: bool,
}

/// Render the standalone login page. `error` is shown when present.
pub fn login_page(error: Option<&str>) -> Markup {
    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { "OpenPanel — Log in" }
                link rel="stylesheet" href="/assets/app.css";
            }
            body {
                main class="login" {
                    h1 { "OpenPanel" }
                    @if let Some(msg) = error {
                        p class="error" { (msg) }
                    }
                    form method="post" action="/login" {
                        label { "Username or email" }
                        input type="text" name="username_or_email" required;
                        label { "Password" }
                        input type="password" name="password" required;
                        button type="submit" { "Log in" }
                    }
                }
            }
        }
    }
}

/// GET /login — show the login form.
pub async fn login_page_handler() -> Markup {
    login_page(None)
}

/// POST /login — authenticate and set the session cookie, or hand off to
/// the factor page when the user has a second factor enrolled.
pub async fn login_handler(
    State(state): State<WebState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Form(form): Form<LoginForm>,
) -> Response {
    let peer = peer.ip();
    let forwarded = forwarded_ip(&headers);
    match state
        .login_throttle
        .check(&form.username_or_email, peer, forwarded)
        .await
    {
        Ok(Some(decision)) => return denied(decision.retry_after_seconds),
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        Ok(None) => {}
    }
    let ip = Some(peer.to_string());
    let ua = user_agent(&headers);
    let remember = remember_cookie_from_headers(&headers);
    match state
        .identity
        .login(
            &form.username_or_email,
            &form.password,
            ip,
            ua,
            remember.as_deref(),
        )
        .await
    {
        Ok(LoginOutcome::Authenticated { user: _user, token }) => {
            if state
                .login_throttle
                .record_success(&form.username_or_email)
                .await
                .is_err()
            {
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
            let mut resp = Redirect::to("/").into_response();
            let cookie = format!(
                "{SESSION_COOKIE}={}; HttpOnly; Path=/; SameSite=Lax; Max-Age=86400",
                token.expose()
            );
            if let Ok(value) = HeaderValue::from_str(&cookie) {
                resp.headers_mut()
                    .insert(axum::http::header::SET_COOKIE, value);
            }
            resp
        }
        Ok(LoginOutcome::FactorRequired {
            user: _user,
            challenge,
        }) => {
            if state
                .login_throttle
                .record_success(&form.username_or_email)
                .await
                .is_err()
            {
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
            login_factor_page(
                challenge.challenge_id.to_string(),
                &challenge.challenge_token,
                challenge.expires_at,
                None,
            )
            .into_response()
        }
        Err(_) => match state
            .login_throttle
            .record_failure(&form.username_or_email, peer, forwarded)
            .await
        {
            Ok(decision) => denied(decision.retry_after_seconds),
            Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        },
    }
}

/// Render the second-factor form, optionally carrying a `code`-flavored
/// error message.
pub fn login_factor_page(
    challenge_id: String,
    challenge_token: &str,
    expires_at: chrono::DateTime<chrono::Utc>,
    error: Option<&str>,
) -> Markup {
    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { "OpenPanel — Second factor" }
                link rel="stylesheet" href="/assets/app.css";
            }
            body {
                main class="login" {
                    h1 { "Second factor required" }
                    @if let Some(msg) = error {
                        p class="error" { (msg) }
                    }
                    p { "Enter the 6-digit code from your authenticator app, or a recovery code if you no longer have the device." }
                    form method="post" action="/login/factor" {
                        input type="hidden" name="challenge_id" value=(challenge_id);
                        input type="hidden" name="challenge_token" value=(challenge_token);
                        label { "TOTP code or recovery code" }
                        input type="text" name="code" required autofocus;
                        label class="field-inline" {
                            input type="checkbox" name="remember" value="true";
                            "Remember this device for 30 days"
                        }
                        button type="submit" { "Verify" }
                    }
                    p class="muted" { "Expires at " (expires_at.to_rfc3339()) }
                }
            }
        }
    }
}

/// POST /login/factor — verify the factor response and set the session cookie.
pub async fn login_factor_handler(
    State(state): State<WebState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Form(form): Form<FactorForm>,
) -> Response {
    let ip = Some(peer.ip().to_string());
    let ip_for_remember = ip.clone();
    let ua = user_agent(&headers);
    let ua_for_remember = ua.clone();
    let kind = form.kind.trim();
    let response = match kind {
        "" | "totp" => FactorResponse::Totp(form.code.clone()),
        "recovery" => FactorResponse::Recovery(form.code.clone()),
        _ => {
            return login_factor_page(
                form.challenge_id.clone(),
                &form.challenge_token,
                chrono::Utc::now() + chrono::Duration::minutes(5),
                Some("Unknown factor kind. Use 'totp' or 'recovery'."),
            )
            .into_response();
        }
    };
    let challenge_id = match uuid::Uuid::parse_str(&form.challenge_id) {
        Ok(value) => value,
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };
    match state
        .identity
        .verify_login_factor_with_factor(challenge_id, &form.challenge_token, response, ip, ua)
        .await
    {
        Ok((_user, factor_id, token)) => {
            let mut resp = Redirect::to("/").into_response();
            let cookie = format!(
                "{SESSION_COOKIE}={}; HttpOnly; Path=/; SameSite=Lax; Max-Age=86400",
                token.expose()
            );
            if let Ok(value) = HeaderValue::from_str(&cookie) {
                resp.headers_mut()
                    .insert(axum::http::header::SET_COOKIE, value);
            }
            // Issue a remember-device cookie when requested and the
            // factor is known (recovery codes are account-scoped, so
            // they don't bind a per-factor cookie).
            if form.remember
                && let Some(factor_id) = factor_id
                && let Some(ip_str) = ip_for_remember.as_deref()
                && let Some(ua_str) = ua_for_remember.as_deref()
            {
                let remember = state.two_factor.issue_remember_device_cookie(
                    _user.id(),
                    factor_id,
                    ua_str,
                    ip_str,
                    chrono::Utc::now(),
                );
                let max_age = state.two_factor.remember_device_lifetime_seconds();
                let remember_cookie = format!(
                    "{}={}; HttpOnly; Path=/; SameSite=Lax; Max-Age={}",
                    openpanel_domain::REMEMBER_DEVICE_COOKIE,
                    remember,
                    max_age
                );
                if let Ok(value) = HeaderValue::from_str(&remember_cookie) {
                    resp.headers_mut()
                        .append(axum::http::header::SET_COOKIE, value);
                }
            }
            resp
        }
        Err(_) => login_factor_page(
            form.challenge_id.clone(),
            &form.challenge_token,
            chrono::Utc::now() + chrono::Duration::minutes(5),
            Some("The code did not match. Try again."),
        )
        .into_response(),
    }
}

fn client_ip(headers: &HeaderMap) -> Option<String> {
    headers
        .get("x-forwarded-for")
        .or_else(|| headers.get("x-real-ip"))
        .and_then(|h| h.to_str().ok())
        .map(|s| s.split(',').next().unwrap_or(s).trim().to_string())
}

fn forwarded_ip(headers: &HeaderMap) -> Option<IpAddr> {
    client_ip(headers).and_then(|value| value.parse().ok())
}

fn denied(retry_after: Option<u64>) -> Response {
    let mut response = (
        StatusCode::UNAUTHORIZED,
        login_page(Some("invalid credentials")),
    )
        .into_response();
    if let Some(seconds) = retry_after
        && let Ok(value) = HeaderValue::from_str(&seconds.to_string())
    {
        response.headers_mut().insert(RETRY_AFTER, value);
    }
    response
}

fn user_agent(headers: &HeaderMap) -> Option<String> {
    headers
        .get(axum::http::header::USER_AGENT)
        .and_then(|h| h.to_str().ok())
        .map(|s| s.to_string())
}

/// Extract the remember-device cookie value (if any) from the request.
fn remember_cookie_from_headers(headers: &HeaderMap) -> Option<String> {
    headers
        .get(axum::http::header::COOKIE)
        .and_then(|h| h.to_str().ok())
        .and_then(|raw| {
            for part in raw.split(';') {
                let trimmed = part.trim();
                if let Some(rest) =
                    trimmed.strip_prefix(&format!("{}=", openpanel_domain::REMEMBER_DEVICE_COOKIE))
                {
                    return Some(rest.to_string());
                }
            }
            None
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn login_page_renders_fields() {
        let out = login_page(None).into_string();
        assert!(out.contains("name=\"username_or_email\""));
        assert!(out.contains("type=\"password\""));
        assert!(out.contains("Log in"));
    }

    #[test]
    fn login_page_renders_error() {
        let out = login_page(Some("invalid credentials")).into_string();
        assert!(out.contains("invalid credentials"));
    }
}
