//! Session middleware. Resolves the bearer token (or cookie) to an
//! `(User, Session)` pair via the IdentityService and injects them into
//! request extensions. Route handlers extract via `AuthUser`.

use std::sync::Arc;

use axum::extract::Request;
use axum::http::header::COOKIE;
use axum::middleware::Next;
use axum::response::Response;
use openpanel_app::IdentityService;
use openpanel_domain::{Session, SessionToken, User};

use crate::extract::{AuthSession, AuthSessionExt};

pub const SESSION_COOKIE: &str = "openpanel_session";

pub async fn session_middleware(
    axum::extract::State(svc): axum::extract::State<Arc<IdentityService>>,
    mut req: Request,
    next: Next,
) -> Response {
    let token = extract_token(&req);
    if let Some(tok) = token {
        if let Ok(parsed) = SessionToken::from_string(tok) {
            if let Ok((user, session)) = svc.resolve_session(&parsed).await {
                req.extensions_mut().insert(AuthSession { user, session });
            }
        }
    }
    next.run(req).await
}

fn extract_token(req: &Request) -> Option<String> {
    if let Some(h) = req.headers().get(axum::http::header::AUTHORIZATION) {
        if let Ok(s) = h.to_str() {
            if let Some(rest) = s.strip_prefix("Bearer ") {
                return Some(rest.trim().to_string());
            }
        }
    }
    if let Some(h) = req.headers().get(COOKIE) {
        if let Ok(s) = h.to_str() {
            for part in s.split(';') {
                let part = part.trim();
                if let Some(rest) = part.strip_prefix(&format!("{SESSION_COOKIE}=")) {
                    return Some(rest.to_string());
                }
            }
        }
    }
    None
}

// Extension trait impl so handlers can `req.auth_session()?`.
impl AuthSessionExt for Request {
    fn auth_session(&self) -> Option<AuthSession> {
        self.extensions().get::<AuthSession>().cloned()
    }
}