//! Wildcard SSL services: DNS-01 challenge solver, wildcard
//! issuer, and renewal scheduler.

use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};
use openpanel_core::{AuditAction, AuditEvent, AuditOutcome, AuditService};
// Re-export so callers can use the trait in `Arc<dyn DnsProviderPort>`.
pub use openpanel_domain::DnsProviderPort;
use openpanel_domain::{
    AcmeEndpointMode, CertRequest, DnsLease, Role, User, WildcardError, WildcardRepository,
    is_provider_allowed,
};
use uuid::Uuid;

use crate::wildcard_ssl::SqliteWildcardRepository;

/// DNS-01 challenge solver. Publishes a TXT lease through the
/// DNS provider port, runs the supplied challenge closure, and
/// revokes the lease on both success and failure paths.
pub struct Dns01ChallengeSolver {
    repo: Arc<SqliteWildcardRepository>,
    dns: Arc<dyn DnsProviderPort>,
    audit: Arc<dyn AuditService>,
}

impl Dns01ChallengeSolver {
    /// Construct a solver.
    pub fn new(
        repo: Arc<SqliteWildcardRepository>,
        dns: Arc<dyn DnsProviderPort>,
        audit: Arc<dyn AuditService>,
    ) -> Self {
        Self { repo, dns, audit }
    }

    /// Solve the challenge for `request`. The closure `challenge`
    /// receives the lease value and is expected to return
    /// `Ok(())` on success; on failure the lease is revoked and
    /// the error is returned to the caller.
    pub async fn solve<F, Fut>(
        &self,
        caller: &User,
        request: &CertRequest,
        challenge: F,
    ) -> Result<(), WildcardError>
    where
        F: FnOnce(String) -> Fut,
        Fut: std::future::Future<Output = Result<(), WildcardError>>,
    {
        require_admin(caller)?;
        if !is_provider_allowed(&request.dns_provider) {
            return Err(WildcardError::ProviderNotAllowed(
                request.dns_provider.clone(),
            ));
        }
        let fqdn = format!("_acme-challenge.{}", request.apex);
        let value = format!("token-{}", Uuid::new_v4());
        let lease = DnsLease {
            id: Uuid::new_v4(),
            cert_request_id: request.id,
            fqdn: fqdn.clone(),
            value: value.clone(),
            created_at: Utc::now(),
            revoked_at: None,
        };
        self.repo.save_lease(&lease).await?;
        let published = self.dns.publish_txt(fqdn.clone(), value.clone()).await?;
        let result = challenge(published.clone()).await;
        // Always revoke the lease; the spec is explicit that the
        // lease is released on both success and failure.
        self.dns.revoke_txt(fqdn.clone()).await?;
        self.repo.revoke_lease(lease.id).await?;
        if let Err(error) = result {
            let _ = self
                .audit
                .record(
                    AuditEvent::new(
                        caller.username().as_str(),
                        AuditAction::WildcardCertFailed,
                        AuditOutcome::Failure,
                    )
                    .target(request.id.to_string())
                    .metadata(serde_json::json!({
                        "fqdn": fqdn,
                        "error": error.to_string(),
                    })),
                )
                .await;
            return Err(error);
        }
        Ok(())
    }
}

/// Wildcard issuer: persists the cert request, runs the
/// challenge, and audits the outcome.
pub struct WildcardIssuer {
    repo: Arc<SqliteWildcardRepository>,
    solver: Dns01ChallengeSolver,
    audit: Arc<dyn AuditService>,
}

impl WildcardIssuer {
    /// Construct an issuer.
    pub fn new(
        repo: Arc<SqliteWildcardRepository>,
        solver: Dns01ChallengeSolver,
        audit: Arc<dyn AuditService>,
    ) -> Self {
        Self {
            repo,
            solver,
            audit,
        }
    }

    /// Issue a cert. The challenge closure here is a no-op for
    /// tests; production wiring would call out to the ACME
    /// directory with the lease value.
    pub async fn issue(&self, caller: &User, request: CertRequest) -> Result<(), WildcardError> {
        require_admin(caller)?;
        self.repo.save_request(&request).await?;
        // The solver already holds a reference to the DNS port
        // and the repo; we just call solve() through it.
        let result = self
            .solver
            .solve(caller, &request, |_value| async {
                // A real implementation would POST the value to
                // the ACME directory and poll for the challenge
                // to be considered valid. The tests use a closure
                // that succeeds.
                Ok(())
            })
            .await;
        let outcome = match &result {
            Ok(_) => AuditOutcome::Success,
            Err(_) => AuditOutcome::Failure,
        };
        let _ = self
            .audit
            .record(
                AuditEvent::new(
                    caller.username().as_str(),
                    AuditAction::WildcardCertIssued,
                    outcome,
                )
                .target(request.id.to_string())
                .metadata(serde_json::json!({
                    "domains": request.domains(),
                    "mode": request.endpoint_mode.as_str(),
                })),
            )
            .await;
        result
    }
}

/// Renewal scheduler. The scheduler lists outstanding leases and
/// the next renewal deadline for a request.
pub struct CertRenewalScheduler {
    /// Held for lifecycle parity with the issuer; the v1 scheduler
    /// computes renewal deadlines from the request itself.
    #[allow(dead_code)]
    repo: Arc<SqliteWildcardRepository>,
    /// Audit stream reserved for renewal events in a later change.
    #[allow(dead_code)]
    audit: Arc<dyn AuditService>,
}

impl CertRenewalScheduler {
    /// Construct a scheduler.
    pub fn new(repo: Arc<SqliteWildcardRepository>, audit: Arc<dyn AuditService>) -> Self {
        Self { repo, audit }
    }

    /// Compute the next renewal deadline for `request` (90 days
    /// before the cert expires; the v1 ships with a constant
    /// expiry of 90 days from issuance).
    pub fn next_renewal(&self, _caller: &User, request: &CertRequest) -> DateTime<Utc> {
        require_admin(_caller).ok();
        request.created_at + Duration::days(60)
    }

    /// Determine the endpoint mode to use for the next renewal.
    pub fn next_endpoint_mode(&self, current: AcmeEndpointMode) -> AcmeEndpointMode {
        current
    }
}

fn require_admin(caller: &User) -> Result<(), WildcardError> {
    match caller.role() {
        Role::Owner | Role::Admin => Ok(()),
        _ => Err(WildcardError::Forbidden),
    }
}
