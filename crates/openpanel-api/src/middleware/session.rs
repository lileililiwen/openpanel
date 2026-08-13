//! Session middleware. Resolves the bearer token (or cookie) to an
//! `(User, Session)` pair via the IdentityService and injects them into
//! request extensions. Route handlers extract via `AuthUser`.

use std::sync::Arc;

use axum::{
    extract::{ConnectInfo, Request},
    http::{StatusCode, header::COOKIE},
    middleware::Next,
    response::{IntoResponse, Response},
};
use openpanel_app::{ApiTokenService, IdentityService, api_tokens::TokenAuthError};
use openpanel_domain::{SessionBuilder, SessionToken, TokenScope};

use crate::extract::{AuthSession, AuthSessionExt};

/// Name of the HTTP cookie that carries the session token for browser clients.
pub const SESSION_COOKIE: &str = "openpanel_session";

/// Axum middleware that resolves the bearer token (or `SESSION_COOKIE` cookie) into
/// an [`AuthSession`] and stores it in the request extensions for downstream extractors.
pub async fn session_middleware(
    axum::extract::State(svc): axum::extract::State<Arc<IdentityService>>,
    mut req: Request,
    next: Next,
) -> Response {
    let token = extract_token(&req);
    if let Some(tok) = token
        && let Ok(parsed) = SessionToken::from_string(tok)
        && let Ok((user, session)) = svc.resolve_session(&parsed).await
    {
        req.extensions_mut().insert(AuthSession {
            user,
            session,
            token_id: None,
        });
    }
    next.run(req).await
}

/// API authentication state combining interactive sessions and scoped bearers.
#[derive(Clone)]
pub struct ApiAuthState {
    /// Interactive identity sessions.
    pub identity: Arc<IdentityService>,
    /// Personal access token resolver.
    pub tokens: Arc<ApiTokenService>,
}

/// Resolve a scoped personal bearer before falling back to normal session auth.
pub async fn api_auth_middleware(
    axum::extract::State(state): axum::extract::State<ApiAuthState>,
    mut req: Request,
    next: Next,
) -> Response {
    if let Some(plaintext) = personal_bearer(&req) {
        let Some(scope) = required_scope(req.method(), req.uri().path()) else {
            return StatusCode::FORBIDDEN.into_response();
        };
        let peer = req
            .extensions()
            .get::<ConnectInfo<std::net::SocketAddr>>()
            .map_or_else(
                || std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
                |info| info.0.ip(),
            );
        match state.tokens.authenticate(&plaintext, peer, &scope).await {
            Ok(principal) => {
                let now = chrono::Utc::now();
                let session = openpanel_domain::Session::restore(SessionBuilder {
                    id: principal.token_id,
                    user_id: principal.user.id(),
                    token_hash: "[api-token]".into(),
                    role: principal.user.role(),
                    created_at: now,
                    last_seen_at: now,
                    absolute_expires_at: now + chrono::Duration::days(365),
                    source_ip: Some(peer.to_string()),
                    user_agent: None,
                });
                req.extensions_mut().insert(AuthSession {
                    user: principal.user,
                    session,
                    token_id: Some(principal.token_id),
                });
                return next.run(req).await;
            }
            Err(error) => return token_error_response(error),
        }
    }
    let token = extract_token(&req);
    if let Some(tok) = token
        && let Ok(parsed) = SessionToken::from_string(tok)
        && let Ok((user, session)) = state.identity.resolve_session(&parsed).await
    {
        req.extensions_mut().insert(AuthSession {
            user,
            session,
            token_id: None,
        });
    }
    next.run(req).await
}

fn personal_bearer(req: &Request) -> Option<String> {
    req.headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|header| header.to_str().ok())
        .and_then(|header| header.strip_prefix("Bearer "))
        .map(str::trim)
        .filter(|value| value.starts_with("openpanel_pat_"))
        .map(str::to_owned)
}

fn required_scope(method: &axum::http::Method, path: &str) -> Option<TokenScope> {
    let relative = path
        .strip_prefix("/api/v1/")
        .unwrap_or_else(|| path.trim_start_matches('/'));
    let context = relative.split('/').next()?;
    let verb = if context == "cron" && relative.ends_with("/run") {
        "run"
    } else if matches!(
        *method,
        axum::http::Method::GET | axum::http::Method::HEAD | axum::http::Method::OPTIONS
    ) {
        "read"
    } else {
        "write"
    };
    TokenScope::parse(&format!("{context}:{verb}")).ok()
}

fn token_error_response(error: TokenAuthError) -> Response {
    let (status, code, retry) = match error {
        TokenAuthError::Unauthorized | TokenAuthError::Expired => {
            (StatusCode::UNAUTHORIZED, "invalid_token", None)
        }
        TokenAuthError::ScopeRejected
        | TokenAuthError::CidrRejected
        | TokenAuthError::Forbidden => (StatusCode::FORBIDDEN, "forbidden", None),
        TokenAuthError::RateLimited {
            retry_after_seconds,
        } => (
            StatusCode::TOO_MANY_REQUESTS,
            "rate_limited",
            Some(retry_after_seconds),
        ),
        TokenAuthError::Invalid(_) => (StatusCode::BAD_REQUEST, "bad_request", None),
        TokenAuthError::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, "internal", None),
    };
    let mut response = (status, axum::Json(serde_json::json!({"error": code}))).into_response();
    if let Some(seconds) = retry
        && let Ok(value) = axum::http::HeaderValue::from_str(&seconds.to_string())
    {
        response
            .headers_mut()
            .insert(axum::http::header::RETRY_AFTER, value);
    }
    response
}

fn extract_token(req: &Request) -> Option<String> {
    if let Some(h) = req.headers().get(axum::http::header::AUTHORIZATION)
        && let Ok(s) = h.to_str()
        && let Some(rest) = s.strip_prefix("Bearer ")
    {
        return Some(rest.trim().to_string());
    }
    if let Some(h) = req.headers().get(COOKIE)
        && let Ok(s) = h.to_str()
    {
        for part in s.split(';') {
            let part = part.trim();
            if let Some(rest) = part.strip_prefix(&format!("{SESSION_COOKIE}=")) {
                return Some(rest.to_string());
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
