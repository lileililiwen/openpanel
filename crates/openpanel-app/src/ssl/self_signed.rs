//! Self-signed certificate generation via `rcgen`.
//!
//! Used by `SslService::generate_self_signed` for dev / internal
//! services that need TLS but don't need a publicly-trusted cert.

use chrono::{Duration, Utc};
use openpanel_domain::ssl::{
    certificate::{Certificate, KeyType},
    error::SslError,
};
use rcgen::{CertificateParams, KeyPair, PKCS_ECDSA_P256_SHA256};

/// Generate a self-signed certificate for `domain`, valid for
/// `valid_for_days` days starting now. Returns `(cert, plaintext_key_pem)`.
/// The plaintext key is returned alongside the stub cert so the
/// caller can encrypt it before persistence; `Certificate.key_pem`
/// is typed as ciphertext and the plaintext key never escapes this
/// function except via the explicit return value.
pub fn generate(
    domain: impl Into<String>,
    valid_for_days: u32,
) -> Result<(Certificate, String), SslError> {
    let domain = domain.into();
    let key_pair = KeyPair::generate_for(&PKCS_ECDSA_P256_SHA256)
        .map_err(|e| SslError::InvalidKey(format!("rcgen key: {e}")))?;
    let params = CertificateParams::new(vec![domain.clone()])
        .map_err(|e| SslError::InvalidCert(format!("rcgen params: {e}")))?;
    // rcgen 0.13: `self_signed` takes the key as a parameter; no
    // `params.key_pair` field.
    let cert = params
        .self_signed(&key_pair)
        .map_err(|e| SslError::InvalidCert(format!("self-signed: {e}")))?;
    let cert_pem = cert.pem();
    let key_pem = key_pair.serialize_pem();
    let now = Utc::now();
    let valid_to = now + Duration::days(valid_for_days as i64);
    let issuer = "OpenPanel Self-Signed".to_string();

    // `Certificate::new` requires a ciphertext `Vec<u8>`. The
    // service layer will encrypt `key_pem` and overwrite the row's
    // `key_pem` field. Here we mint a stub with empty ciphertext.
    let stub = Certificate::new(
        domain,
        openpanel_domain::ssl::source::CertificateSource::SelfSigned,
        issuer,
        now,
        valid_to,
        KeyType::EcdsaP256,
        cert_pem,
        "",
        Vec::new(),
    )?;
    Ok((stub, key_pem))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_a_parseable_self_signed_cert() {
        let (cert, key_pem) = generate("self-signed.example.com", 365).unwrap();
        assert_eq!(cert.key_type, KeyType::EcdsaP256);
        assert!(cert.cert_pem.contains("BEGIN CERTIFICATE"));
        assert!(key_pem.contains("BEGIN PRIVATE KEY"));
    }

    #[test]
    fn validity_window_is_365_days() {
        let (cert, _) = generate("window.example.com", 365).unwrap();
        let span = cert.valid_to.signed_duration_since(cert.valid_from);
        assert!(span.num_days() >= 364 && span.num_days() <= 366);
    }
}
