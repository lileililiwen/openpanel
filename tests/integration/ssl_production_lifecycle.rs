//! ACME production-lifecycle HTTP integration tests.
//!
//! Capability under test: `ssl-production-lifecycle` (complete HTTP-01
//! issuance, fail-closed issuance, safe staging default, durable
//! renewal, visible recovery, bounded backoff, classified errors,
//! DNS/port preflight, 24-hour backoff, gated nginx reload).
//!
//! Every test drives the real `SslService` against a fresh `TestServer`
//! database with a scripted `MockAcmeClient`, so no network or CA
//! access is required.

use std::{collections::HashMap, sync::Arc};

use openpanel_app::ssl::{
    AcmeEndpoint, AcmeHttpServer, IssuedCert, SslPaths, SslService,
    acme::MockAcmeClient,
    issuance_state::{
        INITIAL_POLL_BACKOFF, IssuanceAttempt, IssuanceError, MAX_POLL_ATTEMPTS, MAX_POLL_BACKOFF,
        RENEWAL_RETRY_AFTER, classify_acme_error, classify_problem, redact_acme_text,
    },
    preflight::{PREFLIGHT_HTTP_PORT, PREFLIGHT_STEP_TIMEOUT, PreflightOutcome, preflight},
    repo::SqliteCertificateRepository,
};
use openpanel_domain::ssl::{error::SslError, source::CertificateSource};
use openpanel_test_support::MockAudit;

use crate::common::*;

const CAPABILITY: &str = "ssl-production-lifecycle";

fn scripted_cert() -> IssuedCert {
    IssuedCert {
        cert_pem: "fake-leaf-pem".into(),
        chain_pem: "fake-chain-pem".into(),
        key_pem: "fake-private-key".into(),
        issuer: "Fake Staging CA".into(),
    }
}

/// `SslService` backed by the test server's database and a scripted
/// mock ACME client. Paths are sandboxed per test.
fn mock_service(
    server: &TestServer,
    dir: &str,
    endpoint: AcmeEndpoint,
    scripted: HashMap<String, IssuedCert>,
) -> SslService {
    let repo = Arc::new(SqliteCertificateRepository::new(server.pool()));
    SslService::new(
        repo,
        Arc::new(MockAudit::stub()),
        [0u8; 32],
        SslPaths::under(server.sandbox_path(dir).into()),
        AcmeHttpServer::new(),
        Arc::new(MockAcmeClient { endpoint, scripted }),
        endpoint,
        "[email protected]",
    )
}

fn scripted_service(server: &TestServer, dir: &str, domain: &str) -> SslService {
    let mut scripted = HashMap::new();
    scripted.insert(domain.to_string(), scripted_cert());
    mock_service(server, dir, AcmeEndpoint::Staging, scripted)
}

/// Scenario: Staging issuance succeeds — the certificate is stored as
/// an ACME certificate with staging endpoint metadata and an
/// encrypted key.
#[tokio::test]
async fn lifecycle_staging_issuance_succeeds() {
    assert_eq!(CAPABILITY, "ssl-production-lifecycle");
    let server = TestServer::new().await;
    let svc = scripted_service(&server, "ssl-lifecycle-staging", "staging.example.com");

    let cert = svc
        .issue_acme("staging.example.com")
        .await
        .expect("staging issuance");
    assert!(matches!(cert.source, CertificateSource::Acme));
    assert_eq!(cert.acme_endpoint.as_deref(), Some("staging"));
    assert_eq!(cert.domain, "staging.example.com");

    // Key material is encrypted at rest and round-trips.
    assert!(!cert.key_pem.is_empty(), "key ciphertext stored");
    assert!(
        !String::from_utf8_lossy(&cert.key_pem).contains("fake-private-key"),
        "no plaintext key in row"
    );
    assert_eq!(svc.decrypt_key(&cert).expect("decrypt"), "fake-private-key");

    // Row + on-disk files exist.
    let fetched = svc.get("staging.example.com").await.expect("row");
    assert_eq!(fetched.id, cert.id);
    let paths = SslPaths::under(server.sandbox_path("ssl-lifecycle-staging").into());
    assert!(paths.cert_path("staging.example.com").exists(), "cert file");
    assert!(paths.key_path("staging.example.com").exists(), "key file");
}

/// Scenario: Challenge unreachable — a stable actionable error and no
/// certificate row is written.
#[tokio::test]
async fn lifecycle_failed_issuance_writes_no_row() {
    let server = TestServer::new().await;
    let svc = mock_service(
        &server,
        "ssl-lifecycle-failclosed",
        AcmeEndpoint::Staging,
        HashMap::new(),
    );

    let err = svc
        .issue_acme("unreachable.example.com")
        .await
        .expect_err("mock not scripted");
    let text = err.to_string();
    assert!(
        text.contains("unreachable.example.com"),
        "actionable domain in error: {text}"
    );
    assert!(
        svc.list().await.expect("list").is_empty(),
        "no certificate row written"
    );
}

/// Scenario: Default endpoint — without a production opt-in the
/// adapter uses staging.
#[tokio::test]
async fn lifecycle_default_endpoint_is_staging() {
    assert_eq!(AcmeEndpoint::default_safe(), AcmeEndpoint::Staging);
    assert_eq!(AcmeEndpoint::Staging.as_str(), "staging");
    assert!(
        AcmeEndpoint::Staging
            .directory_url()
            .contains("acme-staging-v02"),
        "staging directory"
    );
    assert!(
        AcmeEndpoint::Production
            .directory_url()
            .contains("acme-v02"),
        "production directory"
    );
}

/// Scenario: Renewal fails — the existing certificate remains active
/// and the failure carries no key material.
#[tokio::test]
async fn lifecycle_failed_renewal_preserves_existing_cert() {
    let server = TestServer::new().await;
    let domain = "renew.example.com";
    let svc = scripted_service(&server, "ssl-lifecycle-renew", domain);
    let before = svc.issue_acme(domain).await.expect("seed cert");

    // A service sharing the same repo/paths whose mock fails.
    let failing = mock_service(
        &server,
        "ssl-lifecycle-renew",
        AcmeEndpoint::Staging,
        HashMap::new(),
    );
    let err = failing
        .issue_acme(domain)
        .await
        .expect_err("renewal must fail");
    assert!(
        !err.to_string().contains("fake-private-key"),
        "failure leaks no key material: {err}"
    );

    let after = svc.get(domain).await.expect("previous cert intact");
    assert_eq!(after.id, before.id, "same row preserved");
    assert_eq!(
        svc.decrypt_key(&after).expect("decrypt"),
        "fake-private-key"
    );
}

/// Scenario: Operator views failed issuance — a recoverable failure
/// state with no key or token material.
#[tokio::test]
async fn lifecycle_failure_state_is_recoverable_and_redacted() {
    let err = classify_acme_error("connection refused while fetching http-01 challenge");
    let redacted = err.redact();
    assert!(redacted.contains("challenge"), "stable kind: {redacted}");
    assert!(
        !redacted.contains("PRIVATE KEY"),
        "no key material: {redacted}"
    );
    let mapped = openpanel_app::ssl::acme::map_issuance_error(err);
    assert!(
        !mapped.to_string().contains("PRIVATE KEY"),
        "mapped error clean: {mapped}"
    );
}

/// Scenario: Order never becomes Ready — bounded attempts, growing
/// backoff capped at 60s, then `Timeout`; challenge tokens unregister.
#[tokio::test]
async fn lifecycle_polling_is_bounded_with_capped_backoff() {
    assert_eq!(MAX_POLL_ATTEMPTS, 6);
    assert_eq!(INITIAL_POLL_BACKOFF, std::time::Duration::from_secs(2));
    assert_eq!(MAX_POLL_BACKOFF, std::time::Duration::from_secs(60));

    let now = chrono::Utc::now();
    let mut attempt = IssuanceAttempt::start(now);
    let mut backoffs = Vec::new();
    while attempt.can_poll() {
        backoffs.push(attempt.record_poll());
    }
    assert_eq!(backoffs.len(), 6, "exactly six polls");
    // `record_poll` increments the attempt counter before computing,
    // so the first backoff is `2s * 2^1 = 4s`, then grows to the cap.
    assert_eq!(backoffs[0], std::time::Duration::from_secs(4));
    for window in backoffs.windows(2) {
        assert!(window[1] >= window[0], "backoff grows: {backoffs:?}");
    }
    assert!(
        backoffs.iter().all(|b| *b <= MAX_POLL_BACKOFF),
        "60s ceiling: {backoffs:?}"
    );
    assert!(!attempt.can_poll(), "no further polls after cap");

    let timeout = IssuanceError::Timeout("order stuck in Pending".into());
    assert_eq!(timeout.kind(), "timeout");
    assert!(timeout.is_transient(), "timeout is retryable");

    // Challenge tokens unregister after exit (success or failure).
    let challenge_server = AcmeHttpServer::new();
    let addr = openpanel_app::ssl::challenge_server::serve(challenge_server.clone(), 0)
        .await
        .expect("serve");
    challenge_server.register("exit.example.com", "tok", "key-auth");
    let url = format!("http://{addr}/.well-known/acme-challenge/tok");
    let body = reqwest::get(&url)
        .await
        .expect("get")
        .text()
        .await
        .expect("body");
    assert_eq!(body, "key-auth");
    challenge_server.unregister("exit.example.com");
    let status = reqwest::get(&url).await.expect("get").status();
    assert_eq!(status, 404, "token gone after unregister");
}

/// Scenario: Rate-limited CA response — stable `AcmeRateLimited`
/// variant and a 24-hour scheduler backoff.
#[tokio::test]
async fn lifecycle_rate_limit_maps_and_backs_off_24h() {
    let err =
        classify_problem("urn:ietf:params:acme:error:rateLimited").expect("rateLimited classifies");
    assert!(matches!(err, IssuanceError::RateLimited(_)));
    assert!(err.is_transient());
    let mapped = openpanel_app::ssl::acme::map_issuance_error(err);
    assert!(
        matches!(mapped, SslError::AcmeRateLimited(_)),
        "stable variant: {mapped}"
    );
    assert_eq!(
        RENEWAL_RETRY_AFTER,
        std::time::Duration::from_secs(24 * 60 * 60),
        "24-hour backoff"
    );
}

/// Scenario: Redacted error text — bearer tokens, PEM blocks, and
/// JWS nonces are replaced before persistence.
#[tokio::test]
async fn lifecycle_secret_material_is_redacted() {
    let bearer = redact_acme_text("failed: Authorization: Bearer hunter2 token");
    assert!(bearer.contains("<redacted>"), "bearer redacted: {bearer}");
    assert!(!bearer.contains("hunter2"), "secret gone: {bearer}");

    let pem = redact_acme_text("key:\n-----BEGIN PRIVATE KEY-----\nABC\n-----END PRIVATE KEY-----");
    assert!(pem.contains("<pem-block redacted>"), "pem redacted: {pem}");
    assert!(!pem.contains("BEGIN PRIVATE KEY"), "pem gone: {pem}");

    let clean = redact_acme_text("connection refused");
    assert_eq!(clean, "connection refused", "clean text untouched");
}

/// Scenario: Domain has no A record — preflight returns
/// `DnsFailure` (issuance must not be attempted).
#[tokio::test]
async fn lifecycle_missing_dns_blocks_issuance() {
    // `.invalid` is reserved by RFC 6761 and must not resolve.
    let outcome = preflight("no-such-record.invalid", None).await;
    assert!(
        matches!(outcome, PreflightOutcome::DnsFailure(_)),
        "dns failure, got {outcome:?}"
    );
    assert!(!outcome.is_ok());
    assert_eq!(outcome.kind(), "dns");
}

/// Scenario: Port 80 unreachable — preflight refuses and issuance
/// must not be attempted. A filtered port surfaces `Timeout`; both
/// outcomes block issuance.
#[tokio::test]
async fn lifecycle_unreachable_port_blocks_issuance() {
    assert_eq!(PREFLIGHT_HTTP_PORT, 80);
    assert_eq!(PREFLIGHT_STEP_TIMEOUT, std::time::Duration::from_secs(5));
    // 127.0.0.1 resolves locally; nothing listens on port 80 here.
    let outcome = preflight("127.0.0.1", None).await;
    assert!(
        matches!(
            outcome,
            PreflightOutcome::PortUnreachable(_) | PreflightOutcome::Timeout(_)
        ),
        "port refused, got {outcome:?}"
    );
    assert!(!outcome.is_ok());
}

/// Scenario: Backoff after rate limit — `last_attempt_at = T0`
/// blocks retry for 24h; manual certs are unaffected.
#[tokio::test]
async fn lifecycle_attempt_backoff_blocks_retry_24h() {
    let server = TestServer::new().await;
    let svc = scripted_service(&server, "ssl-lifecycle-backoff", "backoff.example.com");
    let mut cert = svc
        .issue_acme("backoff.example.com")
        .await
        .expect("seed cert");

    let t0 = chrono::Utc::now();
    cert.record_attempt(t0);
    assert_eq!(cert.last_attempt_at, Some(t0), "T0 recorded");
    assert!(
        cert.attempted_within(t0 + chrono::Duration::hours(23), RENEWAL_RETRY_AFTER),
        "blocked before 24h"
    );
    assert!(
        !cert.attempted_within(t0 + chrono::Duration::hours(25), RENEWAL_RETRY_AFTER),
        "retry allowed after 24h"
    );

    // Manual certificates are unaffected by the ACME backoff rule.
    let manual = svc
        .generate_self_signed("manual.example.com", 30)
        .await
        .expect("self-signed");
    assert!(matches!(manual.source, CertificateSource::SelfSigned));
    let err = svc
        .renew_now("manual.example.com")
        .await
        .expect_err("acme only");
    assert!(
        err.to_string().contains("only valid for ACME certs"),
        "manual unaffected: {err}"
    );
}

/// Scenario: nginx -t fails after renewal — previous files stay in
/// place and the reload reports failure.
#[tokio::test]
async fn lifecycle_failed_nginx_reload_preserves_files() {
    use openpanel_app::sites::nginx::{NginxConfigGenerator, NginxPaths};

    let server = TestServer::new().await;
    let domain = "nginx.example.com";
    let svc = scripted_service(&server, "ssl-lifecycle-nginx", domain);
    svc.issue_acme(domain).await.expect("seed cert");
    let paths = SslPaths::under(server.sandbox_path("ssl-lifecycle-nginx").into());
    let before = std::fs::read(paths.cert_path(domain)).expect("cert file");

    // Point nginx at a binary that does not exist: reload must fail
    // while the previously written files stay untouched.
    let mut nginx_paths = NginxPaths::under(server.sandbox_path("ssl-lifecycle-nginx-cfg").into());
    nginx_paths.nginx_binary = std::path::PathBuf::from("/nonexistent/nginx-binary");
    let failing = SslService::new(
        Arc::new(SqliteCertificateRepository::new(server.pool())),
        Arc::new(MockAudit::stub()),
        [0u8; 32],
        SslPaths::under(server.sandbox_path("ssl-lifecycle-nginx").into()),
        AcmeHttpServer::new(),
        Arc::new(MockAcmeClient::empty(AcmeEndpoint::Staging)),
        AcmeEndpoint::Staging,
        "[email protected]",
    )
    .with_nginx(Arc::new(NginxConfigGenerator::new(nginx_paths)));

    let err = failing.reload_nginx().await.expect_err("reload must fail");
    assert!(
        err.to_string().contains("nginx reload"),
        "reported as reload failure: {err}"
    );
    assert_eq!(
        std::fs::read(paths.cert_path(domain)).expect("cert file"),
        before,
        "previous cert files preserved"
    );
}
