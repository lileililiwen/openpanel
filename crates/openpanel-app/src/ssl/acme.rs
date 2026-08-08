//! ACME client abstraction.
//!
//! The trait [`AcmeClient`] is what `SslService` depends on. A mock
//! implementation [`MockAcmeClient`] ships with the codebase so the
//! service is testable without network access. A real
//! [`RustlsAcmeClient`] that drives Let's Encrypt via `rustls-acme`
//! is provided as a *skeleton* — the 0.13 API surface is intricate
//! enough that a follow-up change will complete the wiring against
//! the exact vendored version. The shape (trait + mock + skeleton) is
//! locked in here so the follow-up is mechanical.

use async_trait::async_trait;
use openpanel_domain::ssl::error::SslError;

use super::challenge_server::AcmeHttpServer;

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

/// Real `rustls-acme` adapter (skeleton).
///
/// The full HTTP-01 issuance lifecycle is non-trivial (directory
/// discovery, account creation, order + auth + http-01 challenge
/// polling, finalize, certificate fetch). A follow-up change
/// completes the wiring against the exact `rustls-acme 0.13` API.
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
        // TODO: drive the full HTTP-01 flow against `rustls-acme 0.13`.
        // See the spec's ACME requirement. The skeleton is intentionally
        // a stub so the rest of the change (domain, service, nginx
        // render, API, CLI) can land without depending on the exact
        // rustls-acme API surface, which has shifted across versions.
        Err(SslError::Acme(
            "RustlsAcmeClient is a skeleton — see follow-up change \
             for the rustls-acme 0.13 wiring"
                .into(),
        ))
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
