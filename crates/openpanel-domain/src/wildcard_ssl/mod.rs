//! Wildcard SSL with DNS-01 challenge bounded context: cert
//! request with `ChallengeKind::Dns01`, ACME endpoint mode, and
//! a DNS lease lifecycle.
//!
//! The challenge solver revokes the TXT lease on both the success
//! and the failure paths; the binder only listens on
//! `127.0.0.1:9080` (production wiring is a separate adapter).

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::RepoError;

/// Errors raised by the wildcard SSL bounded context.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum WildcardError {
    /// The caller is not authorised.
    #[error("forbidden")]
    Forbidden,
    /// The provider is not allowed.
    #[error("provider not allowed: {0}")]
    ProviderNotAllowed(String),
    /// The DNS lease is missing.
    #[error("lease not found: {0}")]
    LeaseNotFound(String),
    /// The ACME endpoint is not configured for this mode.
    #[error("endpoint not configured: {0}")]
    EndpointNotConfigured(String),
    /// The challenge failed.
    #[error("challenge failed: {0}")]
    ChallengeFailed(String),
    /// The cert request is invalid.
    #[error("invalid cert request: {0}")]
    InvalidRequest(String),
    /// Persistence failed.
    #[error("persistence failed: {0}")]
    Persistence(String),
}

impl From<WildcardError> for RepoError {
    fn from(error: WildcardError) -> Self {
        RepoError::new(error.to_string())
    }
}

impl From<RepoError> for WildcardError {
    fn from(error: RepoError) -> Self {
        WildcardError::Persistence(error.0)
    }
}

/// ACME challenge kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChallengeKind {
    /// HTTP-01 challenge (legacy default).
    Http01,
    /// DNS-01 challenge (used for wildcard certs).
    Dns01,
}

impl ChallengeKind {
    /// Stable lower-case label.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Http01 => "http_01",
            Self::Dns01 => "dns_01",
        }
    }
}

/// ACME endpoint mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AcmeEndpointMode {
    /// Let's Encrypt staging.
    Staging,
    /// Let's Encrypt production.
    Production,
}

impl AcmeEndpointMode {
    /// Stable lower-case label.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Staging => "staging",
            Self::Production => "production",
        }
    }

    /// Default endpoint for this mode.
    pub fn endpoint(&self) -> &'static str {
        match self {
            Self::Staging => "https://acme-staging-v02.api.letsencrypt.org/directory",
            Self::Production => "https://acme-v02.api.letsencrypt.org/directory",
        }
    }
}

/// Cert request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CertRequest {
    /// Stable id.
    pub id: Uuid,
    /// Owning site id.
    pub site_id: Uuid,
    /// Apex domain.
    pub apex: String,
    /// Whether the request is a wildcard (apex + `*.apex`).
    pub wildcard: bool,
    /// Challenge kind.
    pub challenge: ChallengeKind,
    /// ACME endpoint mode.
    pub endpoint_mode: AcmeEndpointMode,
    /// DNS provider name.
    pub dns_provider: String,
    /// When the request was created.
    pub created_at: DateTime<Utc>,
}

impl CertRequest {
    /// Construct a new request; rejects wildcard requests that
    /// use HTTP-01 (RFC 8555 requires DNS-01 for wildcards).
    pub fn new(
        site_id: Uuid,
        apex: impl Into<String>,
        wildcard: bool,
        challenge: ChallengeKind,
        endpoint_mode: AcmeEndpointMode,
        dns_provider: impl Into<String>,
    ) -> Result<Self, WildcardError> {
        if wildcard && !matches!(challenge, ChallengeKind::Dns01) {
            return Err(WildcardError::InvalidRequest(
                "wildcard requires DNS-01".into(),
            ));
        }
        Ok(Self {
            id: Uuid::new_v4(),
            site_id,
            apex: apex.into(),
            wildcard,
            challenge,
            endpoint_mode,
            dns_provider: dns_provider.into(),
            created_at: Utc::now(),
        })
    }

    /// The list of domains the issued cert will cover.
    pub fn domains(&self) -> Vec<String> {
        if self.wildcard {
            vec![self.apex.clone(), format!("*.{}", self.apex)]
        } else {
            vec![self.apex.clone()]
        }
    }
}

/// DNS-01 TXT lease stored while the challenge is in flight.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DnsLease {
    /// Stable id.
    pub id: Uuid,
    /// Owning cert request.
    pub cert_request_id: Uuid,
    /// The `_acme-challenge` subdomain (e.g. `_acme-challenge.example.com`).
    pub fqdn: String,
    /// TXT value the ACME server expects to read.
    pub value: String,
    /// When the lease was created.
    pub created_at: DateTime<Utc>,
    /// When the lease was revoked (or `None` while still active).
    pub revoked_at: Option<DateTime<Utc>>,
}

impl DnsLease {
    /// Whether the lease is still active.
    pub fn is_active(&self) -> bool {
        self.revoked_at.is_none()
    }
}

/// Persistence port for the wildcard SSL bounded context.
#[async_trait]
pub trait WildcardRepository: Send + Sync + 'static {
    /// Persist a cert request.
    async fn save_request(&self, request: &CertRequest) -> Result<(), RepoError>;
    /// Load a cert request by id.
    async fn get_request(&self, id: Uuid) -> Result<Option<CertRequest>, RepoError>;

    /// Persist a DNS lease.
    async fn save_lease(&self, lease: &DnsLease) -> Result<(), RepoError>;
    /// Mark a lease as revoked.
    async fn revoke_lease(&self, id: Uuid) -> Result<(), RepoError>;
    /// Look up a lease by FQDN.
    async fn find_lease_by_fqdn(&self, fqdn: &str) -> Result<Option<DnsLease>, RepoError>;
}

/// Port the challenge solver uses to publish / revoke a DNS TXT
/// record. Production wiring points this at a real DNS provider;
/// tests use a `RecordingDnsProvider`.
#[async_trait::async_trait]
pub trait DnsProviderPort: Send + Sync + 'static {
    /// Publish a TXT record; returns the published value.
    async fn publish_txt(
        &self,
        fqdn: String,
        value: String,
    ) -> Result<String, WildcardError>;
    /// Revoke a TXT record.
    async fn revoke_txt(&self, fqdn: String) -> Result<(), WildcardError>;
}

/// Recording DNS provider used by tests. Each call records its
/// action and the value.
pub struct RecordingDnsProvider {
    publishes: std::sync::Mutex<Vec<(String, String)>>,
    revokes: std::sync::Mutex<Vec<String>>,
}

impl RecordingDnsProvider {
    /// Construct an empty recorder.
    pub fn new() -> Self {
        Self {
            publishes: std::sync::Mutex::new(Vec::new()),
            revokes: std::sync::Mutex::new(Vec::new()),
        }
    }
    /// Snapshot publishes.
    pub fn publishes(&self) -> Vec<(String, String)> {
        self.publishes.lock().expect("publishes").clone()
    }
    /// Snapshot revokes.
    pub fn revokes(&self) -> Vec<String> {
        self.revokes.lock().expect("revokes").clone()
    }
}

impl Default for RecordingDnsProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl DnsProviderPort for RecordingDnsProvider {
    async fn publish_txt(
        &self,
        fqdn: String,
        value: String,
    ) -> Result<String, WildcardError> {
        self.publishes
            .lock()
            .expect("publishes")
            .push((fqdn.clone(), value.clone()));
        Ok(value)
    }

    async fn revoke_txt(&self, fqdn: String) -> Result<(), WildcardError> {
        self.revokes.lock().expect("revokes").push(fqdn);
        Ok(())
    }
}

/// Default DNS provider allow-list. New providers MUST be added
/// here; the challenge solver refuses every other name.
pub const ALLOWED_DNS_PROVIDERS: &[&str] = &["route53", "cloudflare", "digitalocean", "hetzner"];

/// The default challenge server bind address (loopback).
pub const CHALLENGE_SERVER_BIND: &str = "127.0.0.1:9080";

/// Whether a provider name is in the default allow-list.
pub fn is_provider_allowed(name: &str) -> bool {
    ALLOWED_DNS_PROVIDERS.iter().any(|p| *p == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wildcard_requires_dns_01() {
        let res = CertRequest::new(
            Uuid::new_v4(),
            "example.com",
            true,
            ChallengeKind::Http01,
            AcmeEndpointMode::Staging,
            "route53",
        );
        assert!(matches!(res, Err(WildcardError::InvalidRequest(_))));
    }

    #[test]
    fn wildcard_request_lists_apex_and_wildcard() {
        let req = CertRequest::new(
            Uuid::new_v4(),
            "example.com",
            true,
            ChallengeKind::Dns01,
            AcmeEndpointMode::Staging,
            "route53",
        )
        .expect("req");
        assert_eq!(
            req.domains(),
            vec!["example.com".to_string(), "*.example.com".to_string()]
        );
    }

    #[test]
    fn default_endpoint_is_staging() {
        assert!(AcmeEndpointMode::Staging.endpoint().contains("staging"));
    }
}
