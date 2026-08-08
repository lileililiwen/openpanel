//! Per-session CSRF tokens and the `ValidateCsrf` extractor.
//!
//! Every state-changing web POST must carry a hidden `_csrf` field matching the
//! token stored for the current session. Tokens are 32 random bytes encoded as
//! base64url. The store is keyed by session id so the token rotates with login
//! and is dropped on logout.

use std::{collections::HashMap, sync::Mutex};

use axum::{
    body::Bytes,
    extract::{FromRequest, Request},
    response::{IntoResponse, Response},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use rand::RngCore;
use uuid::Uuid;

use crate::router::WebState;

/// Number of random bytes backing a CSRF token.
const CSRF_TOKEN_BYTES: usize = 32;

/// A per-session CSRF token.
#[derive(Debug, Clone)]
pub struct CsrfToken(String);

impl CsrfToken {
    /// Generate a fresh 256-bit token.
    pub fn generate() -> Self {
        let mut buf = [0u8; CSRF_TOKEN_BYTES];
        rand::rngs::OsRng.fill_bytes(&mut buf);
        Self(URL_SAFE_NO_PAD.encode(buf))
    }

    /// Return the token's underlying string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// In-memory map of session id -> current CSRF token.
///
/// Note: the design doc places the token in the session row, which needs a
/// schema migration. This in-memory store keeps the web milestone self-contained;
/// it is scoped per-process, matching the API's session store lifetime.
#[derive(Debug, Clone, Default)]
pub struct CsrfStore {
    tokens: std::sync::Arc<Mutex<HashMap<Uuid, String>>>,
}

impl CsrfStore {
    /// Create an empty store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Return the stored token for a session, generating one on first use.
    pub fn token_for(&self, session_id: Uuid) -> String {
        let mut tokens = self.lock();
        if let Some(tok) = tokens.get(&session_id) {
            return tok.clone();
        }
        let tok = CsrfToken::generate().as_str().to_string();
        tokens.insert(session_id, tok.clone());
        tok
    }

    /// Return `true` when the submitted token matches the session's token.
    pub fn verify(&self, session_id: Uuid, submitted: &str) -> bool {
        let tokens = self.lock();
        match tokens.get(&session_id) {
            Some(tok) => tok == submitted,
            None => false,
        }
    }

    /// Drop the stored token for a session (called on logout).
    pub fn remove(&self, session_id: Uuid) {
        let mut tokens = self.lock();
        tokens.remove(&session_id);
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<Uuid, String>> {
        self.tokens
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// Marker extractor that rejects a state-changing POST whose `_csrf` field does
/// not match the token stored for the current session. Yields 403 on mismatch.
pub struct ValidateCsrf;

impl FromRequest<WebState> for ValidateCsrf {
    type Rejection = Response;

    async fn from_request(req: Request, state: &WebState) -> Result<Self, Self::Rejection> {
        let session_id = req
            .extensions()
            .get::<openpanel_api::extract::AuthSession>()
            .map(|auth| auth.session.id());
        let bytes = Bytes::from_request(req, state)
            .await
            .map_err(|_| forbidden())?;
        let submitted = parse_csrf_field(&bytes);
        match (session_id, submitted) {
            (Some(id), Some(tok)) if state.csrf.verify(id, &tok) => Ok(ValidateCsrf),
            _ => Err(forbidden()),
        }
    }
}

fn forbidden() -> Response {
    (axum::http::StatusCode::FORBIDDEN, "invalid csrf token").into_response()
}

/// Parse the `_csrf` field out of a `application/x-www-form-urlencoded` body.
fn parse_csrf_field(body: &Bytes) -> Option<String> {
    let text = std::str::from_utf8(body).ok()?;
    for part in text.split('&') {
        let mut it = part.splitn(2, '=');
        if let (Some(key), Some(value)) = (it.next(), it.next())
            && key == "_csrf"
        {
            return Some(value.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_are_32_random_bytes() {
        let a = CsrfToken::generate();
        let b = CsrfToken::generate();
        assert_ne!(a.as_str(), b.as_str(), "tokens must be random");
        let decoded = URL_SAFE_NO_PAD.decode(a.as_str()).expect("valid base64url");
        assert_eq!(decoded.len(), 32);
    }

    #[test]
    fn store_verifies_matching_token() {
        let store = CsrfStore::new();
        let id = Uuid::new_v4();
        let token = store.token_for(id);
        assert!(store.verify(id, &token), "matching token accepted");
        assert!(!store.verify(id, "wrong-token"), "wrong token rejected");
        assert!(
            !store.verify(Uuid::new_v4(), &token),
            "other session rejected"
        );
        store.remove(id);
        assert!(!store.verify(id, &token), "removed token rejected");
    }

    #[test]
    fn parse_returns_csrf_field() {
        let body = Bytes::from_static(b"_csrf=abc123&other=1");
        assert_eq!(parse_csrf_field(&body).as_deref(), Some("abc123"));
        let empty = Bytes::from_static(b"other=1");
        assert_eq!(parse_csrf_field(&empty), None);
    }
}
