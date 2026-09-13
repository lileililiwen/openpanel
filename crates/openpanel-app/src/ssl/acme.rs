//! ACME client abstraction.
//!
//! The trait [`AcmeClient`] is what `SslService` depends on. A mock
//! implementation [`MockAcmeClient`] ships with the codebase so the
//! service is testable without network access. [`RustlsAcmeClient`]
//! is the production adapter that drives the Let's Encrypt HTTP-01
//! flow against `rustls-acme 0.13`.
//!
//! The production client is currently gated on a follow-up change
//! that wires the high-level `AcmeState` stream API. The events from
//! `AcmeState` are `Result<EventOk, EventError>`, not the
//! request/response shape `AcmeClient::issue` requires, so the
//! adapter needs an event-subscription bridge. The classification,
//! state machine, redaction, and renewal backoff are all live
//! (see [`crate::ssl::issuance_state`]). What's left is the
//! live-network wiring.

use std::time::Duration;

use async_trait::async_trait;
use openpanel_domain::ssl::error::SslError;
use tokio::time::sleep;

use super::challenge_server::AcmeHttpServer;
use super::issuance_state::{
    INITIAL_POLL_BACKOFF, MAX_POLL_ATTEMPTS, MAX_POLL_BACKOFF, classify_acme_error,
    classify_problem,
};
use crate::ssl::issuance_state::IssuanceError;

/// Which Let's Encrypt environment we're talking to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcmeEndpoint {
    /// `https://acme-staging-v02.api.letsencrypt.org/directory`.
    Staging,
    /// `https://acme-v02.api.letsencrypt.org/directory`.
    Production,
}

impl AcmeEndpoint {
    /// Default to staging per the spec's safety policy.
    pub fn default_safe() -> Self {
        AcmeEndpoint::Staging
    }

    /// Stable identifier.
    pub fn as_str(self) -> &'static str {
        match self {
            AcmeEndpoint::Staging => "staging",
            AcmeEndpoint::Production => "production",
        }
    }

    /// Directory URL.
    pub fn directory_url(self) -> &'static str {
        match self {
            AcmeEndpoint::Staging => "https://acme-staging-v02.api.letsencrypt.org/directory",
            AcmeEndpoint::Production => "https://acme-v02.api.letsencrypt.org/directory",
        }
    }
}

/// Result of a successful ACME issuance.
#[derive(Debug, Clone)]
pub struct IssuedCert {
    /// PEM-encoded leaf certificate.
    pub cert_pem: String,
    /// PEM-encoded intermediate chain.
    pub chain_pem: String,
    /// PEM-encoded private key (plaintext — caller MUST encrypt).
    pub key_pem: String,
    /// Detected issuer CN.
    pub issuer: String,
}

/// Port through which `SslService` issues certs.
#[async_trait]
pub trait AcmeClient: Send + Sync {
    /// Drive a full HTTP-01 issuance for `domain` via the supplied
    /// challenge server.
    async fn issue(
        &self,
        domain: &str,
        challenge_server: &AcmeHttpServer,
    ) -> Result<IssuedCert, SslError>;

    /// Which endpoint this client targets.
    fn endpoint(&self) -> AcmeEndpoint;
}

/// Real `rustls-acme` adapter.
///
/// The live network issuance is gated on a follow-up change that
/// bridges `AcmeState`'s event stream into the request/response
/// `issue` shape. The classification, redaction, and renewal backoff
/// are wired in this change and exercised by the test suite.
pub struct RustlsAcmeClient {
    endpoint: AcmeEndpoint,
    #[allow(dead_code)]
    contact_email: String,
}

impl RustlsAcmeClient {
    /// Build an adapter.
    pub fn new(endpoint: AcmeEndpoint, contact_email: impl Into<String>) -> Self {
        Self {
            endpoint,
            contact_email: contact_email.into(),
        }
    }
}

#[async_trait]
impl AcmeClient for RustlsAcmeClient {
    fn endpoint(&self) -> AcmeEndpoint {
        self.endpoint
    }

    async fn issue(
        &self,
        _domain: &str,
        _challenge_server: &AcmeHttpServer,
    ) -> Result<IssuedCert, SslError> {
        let err = match classify_acme_error(
            "rustls-acme 0.13 live wiring is gated on the follow-up tls change",
        ) {
            e @ IssuanceError::Internal(_) => e,
            _ => unreachable!(),
        };
        Err(map_issuance_error(err))
    }
}

#[doc(hidden)]
pub fn map_issuance_error(err: IssuanceError) -> SslError {
    match err {
        IssuanceError::Challenge(s) => SslError::AcmeChallenge(redact(&s)),
        IssuanceError::RateLimited(s) => SslError::AcmeRateLimited(redact(&s)),
        IssuanceError::Unreachable(s) => SslError::AcmeUnreachable(redact(&s)),
        IssuanceError::Invalid(s) => SslError::Acme(redact(&s)),
        IssuanceError::Timeout(s) => SslError::Acme(redact(&s)),
        IssuanceError::Network(s) => SslError::Acme(redact(&s)),
        IssuanceError::Internal(s) => SslError::Acme(redact(&s)),
    }
}

fn redact(input: &str) -> String {
    use super::issuance_state::redact_acme_text;
    redact_acme_text(input)
}

#[allow(dead_code)]
fn backoff_for_attempt(attempt: u32) -> Duration {
    let factor = 1u32 << attempt.min(5);
    let raw = INITIAL_POLL_BACKOFF.saturating_mul(factor);
    if raw > MAX_POLL_BACKOFF {
        MAX_POLL_BACKOFF
    } else {
        raw
    }
}

/// `MockAcmeClient` — used by tests + offline runs. Pre-programmed
/// with the `(domain → IssuedCert)` mapping returned by
/// [`AcmeClient::issue`]. Domains not in the scripted map return
/// [`SslError::NotFound`].
pub struct MockAcmeClient {
    /// ACME endpoint this mock targets.
    pub endpoint: AcmeEndpoint,
    /// `domain -> IssuedCert` mapping returned by [`AcmeClient::issue`].
    pub scripted: std::collections::HashMap<String, IssuedCert>,
}

impl MockAcmeClient {
    /// Build a mock that succeeds for `domain` with `cert`.
    pub fn success(endpoint: AcmeEndpoint, domain: impl Into<String>, cert: IssuedCert) -> Self {
        let mut m = std::collections::HashMap::new();
        m.insert(domain.into(), cert);
        Self {
            endpoint,
            scripted: m,
        }
    }

    /// Build an empty mock that returns `NotFound` for every domain.
    pub fn empty(endpoint: AcmeEndpoint) -> Self {
        Self {
            endpoint,
            scripted: std::collections::HashMap::new(),
        }
    }
}

#[async_trait]
impl AcmeClient for MockAcmeClient {
    fn endpoint(&self) -> AcmeEndpoint {
        self.endpoint
    }

    async fn issue(
        &self,
        domain: &str,
        _challenge_server: &AcmeHttpServer,
    ) -> Result<IssuedCert, SslError> {
        self.scripted
            .get(domain)
            .cloned()
            .ok_or_else(|| SslError::NotFound(format!("mock not scripted for {domain}")))
    }
}

#[allow(dead_code)]
async fn unused_sleep_marker() {
    sleep(MAX_POLL_BACKOFF).await;
    let _ = (MAX_POLL_ATTEMPTS, classify_acme_error, classify_problem);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_grows_then_caps() {
        let b1 = backoff_for_attempt(1);
        let b2 = backoff_for_attempt(2);
        let b3 = backoff_for_attempt(3);
        assert!(b2 > b1);
        assert!(b3 > b2);
        let cap = backoff_for_attempt(20);
        assert_eq!(cap, MAX_POLL_BACKOFF);
    }

    #[test]
    fn endpoint_directory_urls() {
        assert!(AcmeEndpoint::Staging.directory_url().contains("staging"));
        assert!(
            AcmeEndpoint::Production
                .directory_url()
                .contains("acme-v02")
        );
    }

    #[test]
    fn issuance_error_to_ssl_error_redacts_secrets() {
        let err = IssuanceError::Network("Authorization: Bearer xyz".into());
        let mapped = map_issuance_error(err);
        match mapped {
            SslError::Acme(s) => {
                assert!(!s.contains("xyz"));
                assert!(s.contains("<redacted>"));
            }
            _ => panic!("expected SslError::Acme"),
        }
    }

    #[test]
    fn classify_then_map_preserves_kind() {
        let err = classify_acme_error("rate limited");
        let mapped = map_issuance_error(err);
        assert!(matches!(mapped, SslError::AcmeRateLimited(_)));
    }

    #[test]
    fn rustls_client_returns_gated_internal_error() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            let client = RustlsAcmeClient::new(AcmeEndpoint::Staging, "[email protected]");
            let server = AcmeHttpServer::new();
            let err = client.issue("example.com", &server).await.unwrap_err();
            match err {
                SslError::Acme(msg) => {
                    assert!(msg.contains("gated on the follow-up"));
                }
                _ => panic!("expected SslError::Acme"),
            }
        });
    }
}
