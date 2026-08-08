//! ACME HTTP-01 challenge server.
//!
//! Binds to `127.0.0.1:<port>` (NOT `0.0.0.0`) and answers
//! `GET /.well-known/acme-challenge/<token>` with the the
//! `key_authorization` for the currently-active issuance of that
//! domain. nginx port-80 vhosts forward the same path to this
//! server via `proxy_pass http://127.0.0.1:<port>;`.
//!
//! The server holds at most one active challenge per domain — the
//! last `register` wins. That's fine because the issuance lifecycle
//! is serialised by the SslService.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{Arc, RwLock},
};

use axum::{
    Router,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::get,
};
use openpanel_domain::ssl::error::SslError;

/// Inner state behind the challenge server: `domain -> token ->
/// key_authorization`.
#[derive(Debug, Default)]
struct ChallengeMap {
    /// `domain -> (token, key_authorization)`.
    by_domain: HashMap<String, ChallengeEntry>,
}

#[derive(Debug, Clone)]
struct ChallengeEntry {
    token: String,
    key_authorization: String,
}

/// Handle to the challenge server. Cheap to clone (Arc-internals).
#[derive(Clone, Default)]
pub struct AcmeHttpServer {
    inner: Arc<RwLock<ChallengeMap>>,
}

impl AcmeHttpServer {
    /// Create a new server with an empty challenge map.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a challenge token for a domain. The token is opaque;
    /// the key_authorization is what we return on GET.
    pub fn register(
        &self,
        domain: impl Into<String>,
        token: impl Into<String>,
        key_authorization: impl Into<String>,
    ) {
        let mut map = self.inner.write().expect("challenge map poisoned");
        map.by_domain.insert(
            domain.into(),
            ChallengeEntry {
                token: token.into(),
                key_authorization: key_authorization.into(),
            },
        );
    }

    /// Remove the registered challenge for a domain. Called after
    /// issuance completes (success or failure).
    pub fn unregister(&self, domain: &str) {
        let mut map = self.inner.write().expect("challenge map poisoned");
        map.by_domain.remove(domain);
    }

    /// Axum router that serves `/.well-known/acme-challenge/<token>`.
    /// The router is keyed by token, NOT by domain — the challenge
    /// server doesn't need to know which the request is for, just
    /// which token was asked for.
    pub fn router(&self) -> Router {
        let state = self.clone();
        Router::new()
            .route("/.well-known/acme-challenge/{token}", get(serve_challenge))
            .with_state(state)
    }
}

async fn serve_challenge(
    State(server): State<AcmeHttpServer>,
    Path(token): Path<String>,
) -> impl IntoResponse {
    let map = server.inner.read().expect("challenge map poisoned");
    for entry in map.by_domain.values() {
        if entry.token == token {
            return (
                StatusCode::OK,
                [("content-type", "text/plain")],
                entry.key_authorization.clone(),
            );
        }
    }
    (
        StatusCode::NOT_FOUND,
        [("content-type", "text/plain")],
        String::new(),
    )
}

/// Convenience: bind a [`tokio::net::TcpListener`] on
/// `127.0.0.1:port` and serve the challenge router forever. Returns
/// the listener address; the task can be aborted on shutdown.
pub async fn serve(server: AcmeHttpServer, port: u16) -> Result<SocketAddr, SslError> {
    let addr: SocketAddr = ([127, 0, 0, 1], port).into();
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|e| SslError::Io(format!("bind {addr}: {e}")))?;
    let local = listener
        .local_addr()
        .map_err(|e| SslError::Io(format!("local_addr: {e}")))?;
    tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, server.router()).await {
            tracing::error!(error = %e, "ACME challenge server exited");
        }
    });
    Ok(local)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn serves_registered_token() {
        let server = AcmeHttpServer::new();
        server.register("example.com", "tok-abc", "key-auth-xyz");

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, server.router()).await.unwrap();
        });

        let resp = reqwest::get(format!("http://{addr}/.well-known/acme-challenge/tok-abc"))
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        assert_eq!(resp.text().await.unwrap(), "key-auth-xyz");
    }

    #[tokio::test]
    async fn unknown_token_returns_404() {
        let server = AcmeHttpServer::new();
        server.register("example.com", "tok-known", "key-known");

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, server.router()).await.unwrap();
        });

        let resp = reqwest::get(format!(
            "http://{addr}/.well-known/acme-challenge/tok-bogus"
        ))
        .await
        .unwrap();
        assert_eq!(resp.status(), 404);
    }
}
