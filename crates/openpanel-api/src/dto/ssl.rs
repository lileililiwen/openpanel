//! Request/response DTOs for `/api/v1/ssl/*`.
//!
//! Private-key material is NEVER returned in any response. Responses
//! carry only metadata (issuer, valid_from, valid_to, status,
//! force_https, source).

use chrono::{DateTime, Utc};
use openpanel_domain::ssl::{
    certificate::Certificate,
    source::{CertificateSource, CertificateStatus},
};
use serde::{Deserialize, Serialize};

/// Metadata-only view of a `Certificate`. Private keys are never
/// returned by the API.
#[derive(Debug, Serialize)]
pub struct CertificateMetadataDto {
    /// UUID v4 primary key.
    pub id: String,
    /// Domain this certificate covers.
    pub domain: String,
    /// Stable source identifier: `acme`, `manual`, or `self_signed`.
    pub source: &'static str,
    /// Issuer common name (e.g. `Let's Encrypt`).
    pub issuer: String,
    /// Not-before timestamp.
    pub valid_from: DateTime<Utc>,
    /// Not-after timestamp.
    pub valid_to: DateTime<Utc>,
    /// Key algorithm identifier (e.g. `ecdsa-p256`).
    pub key_type: &'static str,
    /// Lifecycle status: `active`, `expiring`, `expired`, `revoked`.
    pub status: &'static str,
    /// Whether the nginx port-80 vhost should 301 to HTTPS.
    pub force_https: bool,
    /// ACME endpoint identifier if `source = acme`.
    pub acme_endpoint: Option<String>,
    /// Row creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Last successful renewal timestamp.
    pub renewed_at: Option<DateTime<Utc>>,
    /// Last issuance/renewal error message, if any.
    pub last_error: Option<String>,
}

impl From<Certificate> for CertificateMetadataDto {
    fn from(cert: Certificate) -> Self {
        let status = cert.status(cert.valid_to);
        Self {
            id: cert.id.to_string(),
            domain: cert.domain,
            source: cert.source.as_str(),
            issuer: cert.issuer,
            valid_from: cert.valid_from,
            valid_to: cert.valid_to,
            key_type: cert.key_type.as_str(),
            status: status.as_str(),
            force_https: cert.force_https,
            acme_endpoint: cert.acme_endpoint,
            created_at: cert.created_at,
            renewed_at: cert.renewed_at,
            last_error: cert.last_error,
        }
    }
}

/// List of metadata.
#[derive(Debug, Serialize)]
pub struct CertificateListResponse {
    /// Every managed certificate's metadata.
    pub certificates: Vec<CertificateMetadataDto>,
}

/// Single-metadata response.
#[derive(Debug, Serialize)]
pub struct CertificateResponse {
    /// The certificate metadata.
    pub certificate: CertificateMetadataDto,
}

/// Request to issue a cert via ACME HTTP-01.
#[derive(Debug, Deserialize)]
pub struct IssueRequest {
    /// Domain to issue for.
    pub domain: String,
    /// Override the default (staging) ACME endpoint. Accepted values:
    /// `"staging"` (default) or `"production"`.
    #[serde(default)]
    pub endpoint: Option<String>,
}

/// Request to upload a manual PEM.
#[derive(Debug, Deserialize)]
pub struct ManualUploadRequest {
    /// Domain this certificate covers.
    pub domain: String,
    /// PEM-encoded leaf certificate.
    pub cert_pem: String,
    /// PEM-encoded intermediate chain (may be empty).
    #[serde(default)]
    pub chain_pem: String,
    /// PEM-encoded private key (encrypted at rest on the server).
    pub key_pem: String,
}

/// Request to generate a self-signed cert.
#[derive(Debug, Deserialize)]
pub struct SelfSignedRequest {
    /// Domain to issue the self-signed cert for.
    pub domain: String,
    /// How many days the cert should be valid.
    #[serde(default = "default_valid_days")]
    pub valid_for_days: u32,
}

fn default_valid_days() -> u32 {
    365
}

/// Toggle the force-HTTPS 301 redirect for a site.
#[derive(Debug, Deserialize)]
pub struct ForceHttpsRequest {
    /// Whether to enable the port-80 to :443 301 redirect.
    pub enabled: bool,
}

// Re-export source enum so handlers can serialise the stable string.
#[allow(dead_code)]
fn _source_marker(_: CertificateSource, _: CertificateStatus) {}
