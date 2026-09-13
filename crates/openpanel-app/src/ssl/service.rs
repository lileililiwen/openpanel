//! `SslService` — the application-layer orchestrator for the ssl
//! bounded context.
//!
//! Responsibilities:
//! - ACME HTTP-01 issuance (delegated to [`AcmeClient`]).
//! - Manual PEM upload with key/cert matching validation.
//! - Self-signed generation.
//! - Listing / fetching / revoking / deleting.
//! - `set_force_https` toggle.
//! - Force-renewal (re-issues regardless of the renewal window).
//! - Drives the renewal scheduler via the `SslRenewalTask` type.

use std::{path::PathBuf, sync::Arc};

use chrono::{Duration, Utc};
use openpanel_core::{AuditAction, AuditEvent, AuditOutcome, AuditService};
use openpanel_domain::ssl::{
    certificate::{Certificate, KeyType},
    error::SslError,
    repository::CertificateRepository,
    source::CertificateSource,
};

use super::{
    super::databases::crypto::KEY_LEN,
    super::sites::nginx::NginxConfigGenerator,
    acme::{AcmeClient, AcmeEndpoint, IssuedCert},
    challenge_server::AcmeHttpServer,
    crypto::{decrypt_key_pem, encrypt_key_pem},
    parser::{NewCertMaterial, new_cert, parse_pem_bundle},
};

/// Filesystem paths used by the ssl service. Mirrors the structure
/// of [`crate::sites::nginx::NginxPaths`] so the same `paths.under`
/// pattern works in tests.
#[derive(Debug, Clone)]
pub struct SslPaths {
    /// Directory where leaf certificate PEMs are written for nginx
    /// (`<domain>.crt`).
    pub cert_dir: PathBuf,
    /// Directory where private keys are written for nginx
    /// (`<domain>.key`, mode 0600). The on-disk PEM here is
    /// plaintext only because nginx has to read it; the DB column
    /// is still ciphertext.
    pub key_dir: PathBuf,
}

impl SslPaths {
    /// Default production paths.
    pub fn default_paths() -> Self {
        Self {
            cert_dir: PathBuf::from("/etc/openpanel/ssl/certs"),
            key_dir: PathBuf::from("/etc/openpanel/ssl/keys"),
        }
    }

    /// Construct paths rooted under an arbitrary directory. Used by
    /// the test harness to sandbox file writes.
    pub fn under(root: PathBuf) -> Self {
        Self {
            cert_dir: root.join("certs"),
            key_dir: root.join("keys"),
        }
    }

    /// Path to the leaf certificate file for a domain.
    pub fn cert_path(&self, domain: &str) -> PathBuf {
        self.cert_dir.join(format!("{domain}.crt"))
    }

    /// Path to the private key file for a domain.
    pub fn key_path(&self, domain: &str) -> PathBuf {
        self.key_dir.join(format!("{domain}.key"))
    }
}

/// Application-layer service for TLS certificates.
pub struct SslService {
    repo: Arc<dyn CertificateRepository>,
    audit: Arc<dyn AuditService>,
    master_key: [u8; KEY_LEN],
    paths: SslPaths,
    challenge_server: AcmeHttpServer,
    acme_client: Arc<dyn AcmeClient>,
    acme_endpoint: AcmeEndpoint,
    contact_email: String,
    nginx: Option<Arc<NginxConfigGenerator>>,
}

impl SslService {
    /// Build a service with the given dependencies. `master_key` is
    /// the 32-byte AES-256-GCM key used to wrap private keys in the
    /// DB; `contact_email` is the Let's Encrypt account contact
    /// (`mailto:[email protected]`); `acme_client` is the ACME
    /// adapter (use `RustlsAcmeClient` in production, `MockAcmeClient`
    /// in tests); `nginx` is optional and enables `reload_nginx` on
    /// successful renewals.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        repo: Arc<dyn CertificateRepository>,
        audit: Arc<dyn AuditService>,
        master_key: [u8; KEY_LEN],
        paths: SslPaths,
        challenge_server: AcmeHttpServer,
        acme_client: Arc<dyn AcmeClient>,
        acme_endpoint: AcmeEndpoint,
        contact_email: impl Into<String>,
    ) -> Self {
        Self {
            repo,
            audit,
            master_key,
            paths,
            challenge_server,
            acme_client,
            acme_endpoint,
            contact_email: contact_email.into(),
            nginx: None,
        }
    }

    /// Attach an nginx manager so `reload_nginx` is a real
    /// `nginx -t && nginx -s reload` after a successful renewal.
    pub fn with_nginx(mut self, nginx: Arc<NginxConfigGenerator>) -> Self {
        self.nginx = Some(nginx);
        self
    }

    /// Resolve the configured ACME endpoint.
    pub fn acme_endpoint(&self) -> AcmeEndpoint {
        self.acme_endpoint
    }

    /// Borrow the challenge server handle.
    pub fn challenge_server(&self) -> &AcmeHttpServer {
        &self.challenge_server
    }

    /// Build an `AcmeClient` configured for this service's endpoint.
    pub fn acme_client(&self) -> Arc<dyn AcmeClient> {
        self.acme_client.clone()
    }

    /// Run the preflight checks for `domain` and return the
    /// outcome. The result is safe to surface to the operator
    /// through the web/API/CLI without further redaction. The
    /// `challenge_loopback` is the `127.0.0.1:<port>` the local
    /// challenge server is bound to; pass `None` to skip the
    /// challenge-routing check.
    pub async fn preflight_status(
        &self,
        domain: &str,
        challenge_loopback: Option<std::net::SocketAddr>,
    ) -> super::preflight::PreflightOutcome {
        super::preflight::preflight(domain, challenge_loopback).await
    }

    /// Reload nginx so the freshly-renewed cert is served. The
    /// `NginxManager` is optional: when it is `None` the call is a
    /// no-op (tests + offline runs).
    pub async fn reload_nginx(&self) -> Result<(), SslError> {
        if let Some(manager) = self.nginx.as_ref() {
            manager
                .reload()
                .map_err(|e| SslError::Io(format!("nginx reload: {e}")))?;
        }
        Ok(())
    }

    /// Fetch every certificate, sorted by domain.
    pub async fn list(&self) -> Result<Vec<Certificate>, SslError> {
        self.repo
            .list()
            .await
            .map_err(|e| SslError::Repo(e.to_string()))
    }

    /// Fetch a single certificate by domain.
    pub async fn get(&self, domain: &str) -> Result<Certificate, SslError> {
        self.repo
            .find_by_domain(domain)
            .await
            .map_err(|e| SslError::Repo(e.to_string()))?
            .ok_or_else(|| SslError::NotFound(domain.to_string()))
    }

    /// Upload a manual PEM (cert + optional chain + private key).
    /// Validates key/cert match, rejects expired certs, encrypts the
    /// private key, writes the cert / key files to disk, and persists
    /// the row.
    pub async fn upload_manual(
        &self,
        domain: &str,
        cert_pem: &str,
        chain_pem: &str,
        key_pem: &str,
    ) -> Result<Certificate, SslError> {
        let bundle = parse_pem_bundle(cert_pem, chain_pem, key_pem)?;
        let key_ct = encrypt_key_pem(&self.master_key, &bundle.key_pem)?;
        let material = NewCertMaterial {
            cert_pem: bundle.cert_pem.clone(),
            chain_pem: bundle.chain_pem.clone(),
            key_pem_plaintext: bundle.key_pem.clone(),
            issuer: bundle.issuer.clone(),
            valid_from: bundle.valid_from,
            valid_to: bundle.valid_to,
            key_type: bundle.key_type,
        };
        let cert = new_cert(
            domain.to_string(),
            CertificateSource::Manual,
            material,
            key_ct,
        )?;
        self.write_files(&cert)?;
        self.repo
            .insert(&cert)
            .await
            .map_err(|e| SslError::Repo(e.to_string()))?;
        self.audit
            .record(
                AuditEvent::new(
                    self.contact_email.as_str(),
                    AuditAction::SslManualUploaded,
                    AuditOutcome::Success,
                )
                .target(domain.to_string())
                .metadata(serde_json::json!({"valid_to": cert.valid_to.to_rfc3339()})),
            )
            .await
            .ok();
        Ok(cert)
    }

    /// Issue via ACME HTTP-01. Drives the full issuance lifecycle
    /// against the configured endpoint.
    pub async fn issue_acme(&self, domain: &str) -> Result<Certificate, SslError> {
        let acme = self.acme_client();
        let issued = acme.issue(domain, &self.challenge_server).await?;
        self.persist_issued(domain, issued).await
    }

    /// Generate a self-signed certificate.
    pub async fn generate_self_signed(
        &self,
        domain: &str,
        valid_for_days: u32,
    ) -> Result<Certificate, SslError> {
        let (stub, key_pem) = super::self_signed::generate(domain, valid_for_days)?;
        let key_ct = encrypt_key_pem(&self.master_key, &key_pem)?;
        let material = NewCertMaterial {
            cert_pem: stub.cert_pem.clone(),
            chain_pem: stub.chain_pem.clone(),
            key_pem_plaintext: key_pem,
            issuer: stub.issuer.clone(),
            valid_from: stub.valid_from,
            valid_to: stub.valid_to,
            key_type: stub.key_type,
        };
        let cert = new_cert(
            domain.to_string(),
            CertificateSource::SelfSigned,
            material,
            key_ct,
        )?;
        self.write_files(&cert)?;
        self.repo
            .insert(&cert)
            .await
            .map_err(|e| SslError::Repo(e.to_string()))?;
        self.audit
            .record(
                AuditEvent::new(
                    self.contact_email.as_str(),
                    AuditAction::SslSelfSignedGenerated,
                    AuditOutcome::Success,
                )
                .target(domain.to_string()),
            )
            .await
            .ok();
        Ok(cert)
    }

    /// Persist an ACME-issued cert to the DB. Returns the row.
    /// If the row already exists (renewal flow), update instead.
    pub async fn persist_issued(
        &self,
        domain: &str,
        issued: IssuedCert,
    ) -> Result<Certificate, SslError> {
        let key_ct = encrypt_key_pem(&self.master_key, &issued.key_pem)?;
        let now = Utc::now();
        let material = NewCertMaterial {
            cert_pem: issued.cert_pem.clone(),
            chain_pem: issued.chain_pem.clone(),
            key_pem_plaintext: issued.key_pem.clone(),
            issuer: issued.issuer.clone(),
            valid_from: now,
            // ACME issues 90-day certs; we mirror Let's Encrypt.
            valid_to: now + Duration::days(90),
            key_type: KeyType::EcdsaP256,
        };
        let mut cert = new_cert(
            domain.to_string(),
            CertificateSource::Acme,
            material,
            key_ct,
        )?;
        cert.acme_endpoint = Some(self.acme_endpoint.as_str().to_string());

        // Renewal path — row exists → update instead of insert.
        let existing = self
            .repo
            .find_by_domain(domain)
            .await
            .map_err(|e| SslError::Repo(e.to_string()))?;
        if let Some(existing) = existing {
            // Renewal path — row exists → update instead.
            cert.id = existing.id;
            cert.created_at = existing.created_at;
            cert.force_https = existing.force_https;
            // Re-parse the freshly-issued cert to get the real
            // valid_to (we don't trust our 90-day default).
            if let Ok(bundle) =
                parse_pem_bundle(&issued.cert_pem, &issued.chain_pem, &issued.key_pem)
            {
                cert.valid_from = bundle.valid_from;
                cert.valid_to = bundle.valid_to;
            }
            self.repo
                .update(&cert)
                .await
                .map_err(|e| SslError::Repo(e.to_string()))?;
        } else {
            if let Ok(bundle) =
                parse_pem_bundle(&issued.cert_pem, &issued.chain_pem, &issued.key_pem)
            {
                cert.valid_from = bundle.valid_from;
                cert.valid_to = bundle.valid_to;
            }
            self.repo
                .insert(&cert)
                .await
                .map_err(|e| SslError::Repo(e.to_string()))?;
        }
        self.write_files(&cert)?;
        self.audit
            .record(
                AuditEvent::new(
                    self.contact_email.as_str(),
                    AuditAction::SslIssued,
                    AuditOutcome::Success,
                )
                .target(domain.to_string())
                .metadata(serde_json::json!({"endpoint": self.acme_endpoint.as_str()})),
            )
            .await
            .ok();
        Ok(cert)
    }

    /// Revoke a certificate: mark + write audit.
    pub async fn revoke(&self, domain: &str) -> Result<Certificate, SslError> {
        let mut cert = self.get(domain).await?;
        cert.mark_revoked();
        cert.last_error = Some("revoked".into());
        self.repo
            .update(&cert)
            .await
            .map_err(|e| SslError::Repo(e.to_string()))?;
        self.audit
            .record(
                AuditEvent::new(
                    self.contact_email.as_str(),
                    AuditAction::SslRevoked,
                    AuditOutcome::Success,
                )
                .target(domain.to_string()),
            )
            .await
            .ok();
        Ok(cert)
    }

    /// Delete a certificate row + remove the on-disk files.
    pub async fn delete(&self, domain: &str) -> Result<(), SslError> {
        let cert = self.get(domain).await?;
        self.repo
            .delete(cert.id)
            .await
            .map_err(|e| SslError::Repo(e.to_string()))?;
        let _ = std::fs::remove_file(self.paths.cert_path(domain));
        let _ = std::fs::remove_file(self.paths.key_path(domain));
        self.audit
            .record(
                AuditEvent::new(
                    self.contact_email.as_str(),
                    AuditAction::SslDeleted,
                    AuditOutcome::Success,
                )
                .target(domain.to_string()),
            )
            .await
            .ok();
        Ok(())
    }

    /// Toggle the per-site force-HTTPS 301 redirect.
    pub async fn set_force_https(&self, domain: &str, on: bool) -> Result<Certificate, SslError> {
        let mut cert = self.get(domain).await?;
        cert.set_force_https(on);
        self.repo
            .update(&cert)
            .await
            .map_err(|e| SslError::Repo(e.to_string()))?;
        self.audit
            .record(
                AuditEvent::new(
                    self.contact_email.as_str(),
                    AuditAction::SslForceHttpsChanged,
                    AuditOutcome::Success,
                )
                .target(domain.to_string())
                .metadata(serde_json::json!({"force_https": on})),
            )
            .await
            .ok();
        Ok(cert)
    }

    /// Force-renew now (regardless of window). Returns the new cert.
    pub async fn renew_now(&self, domain: &str) -> Result<Certificate, SslError> {
        let cert = self.get(domain).await?;
        if !matches!(cert.source, CertificateSource::Acme) {
            return Err(SslError::Acme(format!(
                "renew_now is only valid for ACME certs; {domain} is {}",
                cert.source.as_str()
            )));
        }
        self.issue_acme(domain).await
    }

    /// Decrypt the private key for a domain. Used internally by the
    /// renewal flow / nginx render; the API does NOT expose this.
    pub fn decrypt_key(&self, cert: &Certificate) -> Result<String, SslError> {
        decrypt_key_pem(&self.master_key, &cert.key_pem)
    }

    fn write_files(&self, cert: &Certificate) -> Result<(), SslError> {
        std::fs::create_dir_all(&self.paths.cert_dir).map_err(|e| SslError::Io(e.to_string()))?;
        std::fs::create_dir_all(&self.paths.key_dir).map_err(|e| SslError::Io(e.to_string()))?;
        std::fs::write(self.paths.cert_path(&cert.domain), &cert.cert_pem)
            .map_err(|e| SslError::Io(e.to_string()))?;
        let key = self.decrypt_key(cert)?;
        std::fs::write(self.paths.key_path(&cert.domain), key.as_bytes())
            .map_err(|e| SslError::Io(e.to_string()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(
                self.paths.key_path(&cert.domain),
                std::fs::Permissions::from_mode(0o600),
            );
        }
        Ok(())
    }
}
