//! Auto-renewal scheduler for ACME certificates.
//!
//! Registered on the [`JobSupervisor`](openpanel_core::jobs::JobSupervisor)
//! via `Module::background_tasks` on `SslModule`. Runs as a
//! long-lived task that wakes every 24 hours and re-issues ACME certs
//! whose `valid_to - now <= 30 days`.
//!
//! The scheduler honours a 24-hour backoff after a transient
//! failure: a cert that was attempted today is not retried until the
//! next daily tick. This matches Let's Encrypt's
//! `5-duplicate-certificates-per-week` rate limit and avoids
//! hammering the API during an outage.
//!
//! On a successful renewal the service performs an `nginx -t && nginx -s
//! reload` so the new cert is served immediately. If the test fails
//! the previous on-disk cert files are preserved and the renewal is
//! recorded as failed.

use std::sync::Arc;

use async_trait::async_trait;
use openpanel_core::jobs::BackgroundTask;
use openpanel_domain::ssl::{certificate::Certificate, source::CertificateSource};
use tokio::sync::Notify;

use super::service::SslService;
use crate::ssl::issuance_state::RENEWAL_RETRY_AFTER;

/// Renewal scheduler task. Implements [`BackgroundTask`].
pub struct SslRenewalTask {
    /// Service used to scan the repository and persist renewals.
    pub service: Arc<SslService>,
}

impl SslRenewalTask {
    /// Construct a renewal task bound to the given service.
    pub fn new(service: Arc<SslService>) -> Self {
        Self { service }
    }
}

#[async_trait]
impl BackgroundTask for SslRenewalTask {
    fn name(&self) -> &'static str {
        "ssl-renewal"
    }

    async fn run(self: Box<Self>, shutdown: Arc<Notify>) -> anyhow::Result<()> {
        // Daily renewal loop. Wakes every 24 hours until shutdown.
        let interval = std::time::Duration::from_secs(24 * 60 * 60);
        loop {
            tokio::select! {
                _ = tokio::time::sleep(interval) => {}
                _ = shutdown.notified() => break,
            }
            let now = chrono::Utc::now();
            let certs: Vec<Certificate> = match self.service.list().await {
                Ok(c) => c,
                Err(e) => {
                    tracing::warn!(error = %e, "ssl renewal: list failed");
                    continue;
                }
            };
            let mut renewed = 0usize;
            let mut skipped = 0usize;
            let mut backoff = 0usize;
            let mut failed = 0usize;
            for cert in certs {
                if !matches!(cert.source, CertificateSource::Acme) {
                    skipped += 1;
                    continue;
                }
                if !cert.needs_renewal(now) {
                    continue;
                }
                if cert.attempted_within(now, RENEWAL_RETRY_AFTER) {
                    backoff += 1;
                    tracing::debug!(
                        domain = cert.domain.as_str(),
                        "ssl renewal: backoff (24h since last attempt)"
                    );
                    continue;
                }
                match self.service.issue_acme(&cert.domain).await {
                    Ok(_) => {
                        renewed += 1;
                        if let Err(e) = self.service.reload_nginx().await {
                            tracing::warn!(
                                domain = cert.domain.as_str(),
                                error = %e,
                                "ssl renewal: nginx reload failed; cert is on disk but not yet served"
                            );
                        }
                    }
                    Err(e) => {
                        failed += 1;
                        tracing::warn!(
                            domain = cert.domain.as_str(),
                            error = %e,
                            "ssl renewal failed; will retry next tick",
                        );
                    }
                }
            }
            tracing::info!(
                renewed,
                skipped,
                backoff,
                failed,
                "ssl renewal tick complete"
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use openpanel_domain::ssl::{
        certificate::{Certificate, KeyType},
        source::CertificateSource,
    };

    fn stub_cert(domain: &str, valid_to_offset_days: i64) -> Certificate {
        let now = chrono::Utc::now();
        Certificate {
            id: uuid::Uuid::new_v4(),
            domain: domain.to_string(),
            source: CertificateSource::Acme,
            issuer: "Let's Encrypt".into(),
            valid_from: now,
            valid_to: now + chrono::Duration::days(valid_to_offset_days),
            key_type: KeyType::EcdsaP256,
            cert_pem: String::new(),
            chain_pem: String::new(),
            key_pem: vec![],
            force_https: true,
            acme_endpoint: Some("staging".into()),
            created_at: now,
            renewed_at: None,
            last_error: None,
            last_attempt_at: None,
        }
    }

    #[test]
    fn acme_within_window_needs_renewal() {
        let now = chrono::Utc::now();
        let c = stub_cert("expiring.example.com", 20);
        assert!(c.needs_renewal(now));
    }

    #[test]
    fn acme_outside_window_does_not_need_renewal() {
        let now = chrono::Utc::now();
        let c = stub_cert("fresh.example.com", 60);
        assert!(!c.needs_renewal(now));
    }

    #[test]
    fn manual_cert_never_needs_renewal_even_within_window() {
        let now = chrono::Utc::now();
        let mut c = stub_cert("manual.example.com", 5);
        c.source = CertificateSource::Manual;
        assert!(!c.needs_renewal(now));
    }

    #[test]
    fn attempted_within_window_blocks_renewal() {
        let now = chrono::Utc::now();
        let mut c = stub_cert("retry.example.com", 20);
        c.record_attempt(now);
        assert!(c.attempted_within(now, std::time::Duration::from_secs(24 * 3600)));
        // After 25h, the window has elapsed.
        assert!(!c.attempted_within(
            now + chrono::Duration::hours(25),
            std::time::Duration::from_secs(24 * 3600)
        ));
    }

    #[test]
    fn attempted_within_never_returns_false_when_no_attempts() {
        let now = chrono::Utc::now();
        let c = stub_cert("fresh.example.com", 60);
        assert!(!c.attempted_within(now, std::time::Duration::from_secs(24 * 3600)));
    }
}
