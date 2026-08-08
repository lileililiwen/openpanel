//! Login page and login/logout handlers.
//!
//! Login is the one unauthenticated web route (there is no session to bind a
//! CSRF token to yet), so it accepts a plain form POST. Successful login sets
//! the `openpanel_session` cookie; failures render the page with a 401.

use axum::{
    extract::{Form, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Redirect, Response},
};
use maud::{DOCTYPE, Markup, html};
use openpanel_api::middleware::session::SESSION_COOKIE;
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

/// POST /login — authenticate and set the session cookie.
pub async fn login_handler(
    State(state): State<WebState>,
    headers: HeaderMap,
    Form(form): Form<LoginForm>,
) -> Response {
    let ip = client_ip(&headers);
    let ua = user_agent(&headers);
    match state
        .identity
        .login(&form.username_or_email, &form.password, ip, ua)
        .await
    {
        Ok((_user, token)) => {
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
        Err(e) => (StatusCode::UNAUTHORIZED, login_page(Some(&e.to_string()))).into_response(),
    }
}

fn client_ip(headers: &HeaderMap) -> Option<String> {
    headers
        .get("x-forwarded-for")
        .or_else(|| headers.get("x-real-ip"))
        .and_then(|h| h.to_str().ok())
        .map(|s| s.split(',').next().unwrap_or(s).trim().to_string())
}

fn user_agent(headers: &HeaderMap) -> Option<String> {
    headers
        .get(axum::http::header::USER_AGENT)
        .and_then(|h| h.to_str().ok())
        .map(|s| s.to_string())
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
