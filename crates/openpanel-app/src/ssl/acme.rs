//! ACME client abstraction.
//!
//! The trait [`AcmeClient`] is what `SslService` depends on. A mock
//! implementation [`MockAcmeClient`] ships with the codebase so the
//! service is testable without network access. [`RustlsAcmeClient`]
//! is the production adapter that drives the Let's Encrypt HTTP-01
//! flow against `rustls-acme 0.13`.
//!
//! The adapter bridges the `AcmeState` event stream into the panel's
//! [`AcmeHttpServer`] via a token-lookup hook, so the existing
//! nginx-proxied port-80 topology answers ACME validations without a
//! second server. Issued certificate material is captured through an
//! in-memory [`rustls_acme::Cache`] implementation that persists
//! nothing to disk (fail-closed by construction). The order-driving
//! loop sits behind the [`AcmeOrderDriver`] seam so `issue()` is
//! testable offline with a scripted double.

use std::{
    io,
    sync::{Arc, Mutex},
    time::Duration,
};

use async_trait::async_trait;
use futures_util::StreamExt;
use openpanel_domain::ssl::error::SslError;
use rustls_acme::{AccountCache, AcmeConfig, CertCache, EventError, EventOk, UseChallenge};

use super::challenge_server::{AcmeHttpServer, TokenLookup};
use super::issuance_state::{
    INITIAL_POLL_BACKOFF, MAX_POLL_ATTEMPTS, MAX_POLL_BACKOFF, classify_acme_error,
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

/// Bounding knobs for the [`AcmeState`] event loop. Defaults reuse
/// the shared issuance constants; tests inject sub-second bounds.
#[derive(Debug, Clone, Copy)]
struct PollBounds {
    max_attempts: u32,
    initial: Duration,
}

impl Default for PollBounds {
    fn default() -> Self {
        Self {
            max_attempts: MAX_POLL_ATTEMPTS,
            initial: INITIAL_POLL_BACKOFF,
        }
    }
}

impl PollBounds {
    fn backoff(&self, attempt: u32) -> Duration {
        let factor = 1u32 << attempt.min(5);
        self.initial.saturating_mul(factor).min(MAX_POLL_BACKOFF)
    }
}

/// ACME event item produced by `AcmeState` with the capture cache's
/// error types.
type AcmeEvent = Result<EventOk, EventError<io::Error, io::Error>>;

/// Slot holding the captured certificate PEM written by
/// [`CaptureCache::store_cert`].
type CaptureSlot = Arc<Mutex<Option<Vec<u8>>>>;

/// Seam that drives one ACME order to completion. The real
/// implementation polls `rustls_acme::AcmeState`; offline tests script
/// the event sequence so the event loop and PEM bridging are verifiable
/// without touching Let's Encrypt.
#[async_trait]
pub(crate) trait AcmeOrderDriver: Send + Sync {
    async fn drive(
        &self,
        domain: &str,
        endpoint: AcmeEndpoint,
        contact_email: &str,
        challenge_server: &AcmeHttpServer,
    ) -> Result<IssuedCert, SslError>;
}

/// In-memory `rustls-acme` cache. Certificate material is captured
/// for the panel's own storage and persisted nowhere; the account key
/// stays process-lifetime so renewals reuse one ACME account instead
/// of registering a fresh one per issuance.
#[derive(Clone)]
struct CaptureCache {
    captured: CaptureSlot,
    account: CaptureSlot,
}

#[async_trait]
impl CertCache for CaptureCache {
    type EC = io::Error;

    async fn load_cert(
        &self,
        _domains: &[String],
        _directory_url: &str,
    ) -> Result<Option<Vec<u8>>, Self::EC> {
        Ok(None)
    }

    async fn store_cert(
        &self,
        _domains: &[String],
        _directory_url: &str,
        cert: &[u8],
    ) -> Result<(), Self::EC> {
        let mut slot = self
            .captured
            .lock()
            .map_err(|_| io::Error::other("ACME capture cache poisoned"))?;
        *slot = Some(cert.to_vec());
        Ok(())
    }
}

#[async_trait]
impl AccountCache for CaptureCache {
    type EA = io::Error;

    async fn load_account(
        &self,
        _contact: &[String],
        _directory_url: &str,
    ) -> Result<Option<Vec<u8>>, Self::EA> {
        Ok(self
            .account
            .lock()
            .map_err(|_| io::Error::other("ACME account cache poisoned"))?
            .clone())
    }

    async fn store_account(
        &self,
        _contact: &[String],
        _directory_url: &str,
        account: &[u8],
    ) -> Result<(), Self::EA> {
        let mut slot = self
            .account
            .lock()
            .map_err(|_| io::Error::other("ACME account cache poisoned"))?;
        *slot = Some(account.to_vec());
        Ok(())
    }
}

/// Real driver: builds an `AcmeConfig` for one domain, attaches the
/// resolver's HTTP-01 lookup to the panel challenge server, and runs
/// the bounded event loop.
#[derive(Default)]
struct RustlsOrderDriver {
    bounds: PollBounds,
    account: CaptureSlot,
}

#[async_trait]
impl AcmeOrderDriver for RustlsOrderDriver {
    async fn drive(
        &self,
        domain: &str,
        endpoint: AcmeEndpoint,
        contact_email: &str,
        challenge_server: &AcmeHttpServer,
    ) -> Result<IssuedCert, SslError> {
        let captured: CaptureSlot = Arc::new(Mutex::new(None));
        let state = AcmeConfig::new([domain])
            .contact([contact_email])
            .directory_lets_encrypt(matches!(endpoint, AcmeEndpoint::Production))
            .challenge_type(UseChallenge::Http01)
            .cache(CaptureCache {
                captured: captured.clone(),
                account: self.account.clone(),
            })
            .state();
        let lookup: TokenLookup = {
            let resolver = state.resolver();
            Arc::new(move |token: &str| resolver.get_http_01_key_auth(token))
        };
        challenge_server.attach_token_lookup(lookup);
        run_event_loop(
            Box::pin(state),
            &self.bounds,
            challenge_server,
            &captured,
            domain,
        )
        .await
    }
}

/// Poll the ACME event stream until the certificate is issued and
/// captured, with bounded waits between events. Every exit path
/// detaches the challenge lookup so stale resolvers never answer GETs.
async fn run_event_loop<St>(
    mut events: St,
    bounds: &PollBounds,
    challenge_server: &AcmeHttpServer,
    captured: &CaptureSlot,
    domain: &str,
) -> Result<IssuedCert, SslError>
where
    St: futures_util::Stream<Item = AcmeEvent> + Unpin,
{
    let mut new_cert_seen = false;
    let mut attempt: u32 = 0;
    loop {
        if attempt >= bounds.max_attempts {
            challenge_server.detach_token_lookup();
            return Err(map_issuance_error(IssuanceError::Timeout(format!(
                "ACME order for {domain} stalled: no event within {} polling attempts",
                bounds.max_attempts
            ))));
        }
        let event = match tokio::time::timeout(bounds.backoff(attempt), events.next()).await {
            Err(_elapsed) => {
                attempt += 1;
                continue;
            }
            Ok(None) => {
                challenge_server.detach_token_lookup();
                return Err(map_issuance_error(classify_acme_error(
                    "ACME event stream ended before a certificate was issued",
                )));
            }
            Ok(Some(Ok(event))) => event,
            Ok(Some(Err(error))) => {
                challenge_server.detach_token_lookup();
                return Err(map_issuance_error(classify_acme_error(&error.to_string())));
            }
        };
        attempt = 0;
        match event {
            EventOk::DeployedNewCert => new_cert_seen = true,
            EventOk::CertCacheStore if new_cert_seen => {
                challenge_server.detach_token_lookup();
                let pem = captured
                    .lock()
                    .map_err(|_| SslError::Acme("ACME capture cache poisoned".into()))?
                    .clone()
                    .ok_or_else(|| SslError::Acme("ACME certificate was not captured".into()))?;
                return issued_cert_from_pem(&pem);
            }
            _ => {}
        }
    }
}

/// Split a concatenated PEM blob into its `-----BEGIN/END-----`
/// blocks. rustls-acme stores `[certificate key PEM, "\n", chain
/// PEMs]`, so block 0 is the private key and the rest are
/// certificates.
fn pem_blocks(blob: &[u8]) -> Vec<String> {
    let text = String::from_utf8_lossy(blob);
    let mut blocks = Vec::new();
    let mut current: Option<String> = None;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("-----BEGIN") {
            current = Some(format!("{trimmed}\n"));
        } else if let Some(buffer) = current.as_mut() {
            buffer.push_str(trimmed);
            buffer.push('\n');
            if trimmed.starts_with("-----END") {
                blocks.push(std::mem::take(buffer));
                current = None;
            }
        }
    }
    blocks
}

/// Build an [`IssuedCert`] from a captured rustls-acme PEM bundle.
fn issued_cert_from_pem(pem: &[u8]) -> Result<IssuedCert, SslError> {
    let blocks = pem_blocks(pem);
    if blocks.len() < 2 {
        return Err(SslError::InvalidPem(
            "ACME bundle is missing the private key or certificate PEM block".into(),
        ));
    }
    let key_pem = blocks[0].clone();
    let leaf_pem = blocks[1].clone();
    let chain = &blocks[2..];
    if !key_pem.contains("PRIVATE KEY") || !leaf_pem.contains("CERTIFICATE") {
        return Err(SslError::InvalidPem(
            "ACME bundle has an unexpected PEM layout".into(),
        ));
    }
    Ok(IssuedCert {
        cert_pem: leaf_pem,
        chain_pem: chain.join("\n"),
        key_pem,
        issuer: detect_issuer(&blocks[1])?,
    })
}

/// Extract the issuer DN from a leaf certificate PEM.
fn detect_issuer(leaf_pem: &str) -> Result<String, SslError> {
    let der = rustls_pemfile::certs(&mut leaf_pem.as_bytes())
        .next()
        .and_then(Result::ok)
        .ok_or_else(|| SslError::InvalidPem("no certificate in ACME leaf".into()))?;
    let (_rest, cert) = {
        use x509_parser::prelude::{FromDer, X509Certificate};
        X509Certificate::from_der(der.as_ref())
            .map_err(|e| SslError::InvalidCert(format!("x509 parse: {e}")))?
    };
    Ok(cert.issuer().to_string())
}

/// Real `rustls-acme` adapter.
pub struct RustlsAcmeClient {
    endpoint: AcmeEndpoint,
    contact_email: String,
    driver: Arc<dyn AcmeOrderDriver>,
}

impl RustlsAcmeClient {
    /// Build an adapter driving the real `rustls-acme` state machine.
    pub fn new(endpoint: AcmeEndpoint, contact_email: impl Into<String>) -> Self {
        Self::with_driver(
            endpoint,
            contact_email,
            Arc::new(RustlsOrderDriver::default()),
        )
    }

    /// Build an adapter around a scripted driver (offline tests).
    pub(crate) fn with_driver(
        endpoint: AcmeEndpoint,
        contact_email: impl Into<String>,
        driver: Arc<dyn AcmeOrderDriver>,
    ) -> Self {
        Self {
            endpoint,
            contact_email: contact_email.into(),
            driver,
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
        domain: &str,
        challenge_server: &AcmeHttpServer,
    ) -> Result<IssuedCert, SslError> {
        self.driver
            .drive(domain, self.endpoint, &self.contact_email, challenge_server)
            .await
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Generate a `[key, leaf, intermediate]` PEM bundle in the
    /// layout rustls-acme captures.
    fn sample_bundle() -> Vec<u8> {
        let key_pair =
            rcgen::KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256).expect("key pair");
        let params =
            rcgen::CertificateParams::new(vec!["issued.example.com".to_owned()]).expect("params");
        let leaf = params.self_signed(&key_pair).expect("leaf");
        format!("{}\n{}\n", key_pair.serialize_pem(), leaf.pem()).into_bytes()
    }

    fn script(events: Vec<AcmeEvent>) -> futures_util::stream::Iter<std::vec::IntoIter<AcmeEvent>> {
        futures_util::stream::iter(events)
    }

    #[test]
    fn backoff_grows_then_caps() {
        let bounds = PollBounds::default();
        let b1 = bounds.backoff(1);
        let b2 = bounds.backoff(2);
        let b3 = bounds.backoff(3);
        assert!(b2 > b1);
        assert!(b3 > b2);
        let cap = bounds.backoff(20);
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

    #[tokio::test]
    async fn issued_cert_carries_leaf_chain_and_key() {
        let pem = sample_bundle();
        let cert = issued_cert_from_pem(&pem).expect("issued cert");
        assert!(cert.cert_pem.contains("BEGIN CERTIFICATE"));
        assert!(cert.key_pem.contains("BEGIN PRIVATE KEY"));
        assert!(!cert.issuer.is_empty());
    }

    #[tokio::test]
    async fn issued_cert_rejects_truncated_bundle() {
        let key_pair =
            rcgen::KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256).expect("key pair");
        let err = issued_cert_from_pem(key_pair.serialize_pem().as_bytes()).unwrap_err();
        assert!(matches!(err, SslError::InvalidPem(_)));
    }

    #[tokio::test]
    async fn stalled_order_is_bounded_and_detaches_hook() {
        let server = AcmeHttpServer::new();
        let lookup: TokenLookup = Arc::new(|token| {
            if token == "tok-stalled" {
                Some("stalled-key-auth".to_owned())
            } else {
                None
            }
        });
        server.attach_token_lookup(lookup);

        let captured: CaptureSlot = Arc::new(Mutex::new(Some(sample_bundle())));
        let bounds = PollBounds {
            max_attempts: 2,
            initial: Duration::from_millis(10),
        };
        let stalled = futures_util::stream::pending::<AcmeEvent>();
        let err = run_event_loop(
            std::pin::pin!(stalled),
            &bounds,
            &server,
            &captured,
            "stalled.example.com",
        )
        .await
        .expect_err("stalled order must time out");
        assert!(
            matches!(err, SslError::Acme(_)),
            "timeout must classify into SslError::Acme, got {err:?}"
        );

        // The hook is detached on every terminal path: the delegated
        // lookup no longer answers.
        assert!(
            !server.token_lookup_attached(),
            "timeout must detach the challenge lookup"
        );
    }

    #[tokio::test]
    async fn scripted_events_complete_issuance_and_detach_hook() {
        let server = AcmeHttpServer::new();
        let lookup: TokenLookup = Arc::new(|token| {
            if token == "tok-live" {
                Some("live-key-auth".to_owned())
            } else {
                None
            }
        });
        server.attach_token_lookup(lookup);

        let captured: CaptureSlot = Arc::new(Mutex::new(Some(sample_bundle())));
        let bounds = PollBounds {
            max_attempts: 4,
            initial: Duration::from_millis(10),
        };
        let events = script(vec![
            Ok(EventOk::AccountCacheStore),
            Ok(EventOk::DeployedNewCert),
            Ok(EventOk::CertCacheStore),
        ]);
        let issued = run_event_loop(
            std::pin::pin!(events),
            &bounds,
            &server,
            &captured,
            "issued.example.com",
        )
        .await
        .expect("scripted issuance");
        assert!(issued.cert_pem.contains("BEGIN CERTIFICATE"));
        assert!(issued.key_pem.contains("BEGIN PRIVATE KEY"));
        assert!(!issued.issuer.is_empty());
        assert!(
            !server.token_lookup_attached(),
            "success must detach the challenge lookup"
        );
    }

    #[tokio::test]
    async fn client_delegates_to_driver() {
        struct ScriptedDriver;
        #[async_trait]
        impl AcmeOrderDriver for ScriptedDriver {
            async fn drive(
                &self,
                domain: &str,
                _endpoint: AcmeEndpoint,
                _contact_email: &str,
                _challenge_server: &AcmeHttpServer,
            ) -> Result<IssuedCert, SslError> {
                Ok(IssuedCert {
                    cert_pem: format!("leaf-for-{domain}"),
                    chain_pem: String::new(),
                    key_pem: "key".into(),
                    issuer: "Test CA".into(),
                })
            }
        }
        let client = RustlsAcmeClient::with_driver(
            AcmeEndpoint::Staging,
            "[email protected]",
            Arc::new(ScriptedDriver),
        );
        let issued = client
            .issue("driver.example.com", &AcmeHttpServer::new())
            .await
            .expect("driver issuance");
        assert_eq!(issued.cert_pem, "leaf-for-driver.example.com");
        assert_eq!(issued.issuer, "Test CA");
    }

    #[tokio::test]
    async fn acme_order_error_classifies_into_ssl_error() {
        let server = AcmeHttpServer::new();
        let captured: CaptureSlot = Arc::new(Mutex::new(None));
        let bounds = PollBounds {
            max_attempts: 4,
            initial: Duration::from_millis(10),
        };
        let events = script(vec![Err(EventError::CertCacheLoad(io::Error::other(
            "rate limited by the directory",
        )))]);
        let err = run_event_loop(
            std::pin::pin!(events),
            &bounds,
            &server,
            &captured,
            "err.example.com",
        )
        .await
        .expect_err("event error must surface");
        assert!(
            matches!(err, SslError::AcmeRateLimited(_) | SslError::Acme(_)),
            "got {err:?}"
        );
    }
}
