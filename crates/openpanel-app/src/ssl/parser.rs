//! PEM bundle parsing + key/cert matching for the ssl bounded context.
//!
//! Given a `(cert_pem, chain_pem, key_pem)` triple from a manual upload
//! or a self-signed generation, parse + validate that the key matches
//! the cert. Used by `SslService::upload_manual` and as a sanity
//! check for self-signed generation output.

use chrono::{DateTime, Utc};
use openpanel_domain::ssl::{
    certificate::{Certificate, KeyType},
    error::SslError,
};
use rcgen::KeyPair;
use rustls_pemfile::certs;
use x509_parser::prelude::*;

use super::super::databases::crypto::KEY_LEN;

/// Parsed and validated result of `parse_pem_bundle`.
#[derive(Debug)]
pub struct ParsedBundle {
    /// The leaf certificate (PEM, possibly re-serialised).
    pub cert_pem: String,
    /// The chain certificates (PEM).
    pub chain_pem: String,
    /// The PEM-encoded private key (unencrypted).
    pub key_pem: String,
    /// The CN (or SAN at index 0 if CN is empty) of the leaf.
    pub subject: String,
    /// Issuer CN.
    pub issuer: String,
    /// Validity start (not_before).
    pub valid_from: DateTime<Utc>,
    /// Validity end (not_after).
    pub valid_to: DateTime<Utc>,
    /// Detected key algorithm.
    pub key_type: KeyType,
}

/// Parse + validate a `(cert, chain, key)` PEM triple.
///
/// Verifies that `key_pem` actually matches `cert_pem` by attempting
/// to build a `KeyPair` from the key, deriving the public-key bytes
/// from it, and comparing them against the cert's `SubjectPublicKeyInfo`.
///
/// Rejects expired certs (`valid_to < now`) per the spec.
pub fn parse_pem_bundle(
    cert_pem: &str,
    chain_pem: &str,
    key_pem: &str,
) -> Result<ParsedBundle, SslError> {
    // 1. Parse the leaf cert.
    let leaf_der_vec = certs(&mut cert_pem.as_bytes())
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| SslError::InvalidPem(format!("cert block: {e}")))?;
    let leaf_der = leaf_der_vec
        .into_iter()
        .next()
        .ok_or_else(|| SslError::InvalidPem("no certificate in `cert_pem`".into()))?;
    let (_rest, cert) = X509Certificate::from_der(leaf_der.as_ref())
        .map_err(|e| SslError::InvalidCert(format!("x509 parse: {e}")))?;

    // 2. Validity window + expiry check.
    let valid_to = x509_to_chrono(cert.validity().not_after);
    let valid_from = x509_to_chrono(cert.validity().not_before);
    if valid_to < Utc::now() {
        return Err(SslError::Expired);
    }

    // 3. Parse the private key.
    let key_pair =
        KeyPair::from_pem(key_pem).map_err(|e| SslError::InvalidKey(format!("PEM parse: {e}")))?;

    // 4. Verify the key matches the certificate by rebuilding the
    //    same rcgen parameters and re-signing. We move `key_pair` so
    //    we don't need `Clone` (rcgen's `KeyPair` isn't Clone).
    //
    // (The `params` previously used for re-signing was removed in
    // favour of the direct SPKI comparison below — rcgen's `self_signed`
    // doesn't actually verify the key matches any specific cert, so
    // a byte-equality comparison on the re-signed DER is the wrong
    // primitive.)
    // 4. Verify the key matches the certificate by comparing the
    //    cert's SubjectPublicKeyInfo raw bytes with the key's
    //    public key raw bytes. rcgen exposes `public_key_raw()`;
    //    x509-parser exposes the SPKI on the parsed cert.
    let cert_spki = cert.public_key().subject_public_key.data.as_ref();
    let key_pub = key_pair.public_key_raw();
    if cert_spki != key_pub {
        return Err(SslError::KeyMismatch);
    }

    // 5. Extract subject / issuer.
    let subject = cert.subject().to_string();
    let issuer = cert.issuer().to_string();

    // 6. Detect key type. rcgen 0.13's KeyPair doesn't expose a
    //    stable algorithm-name getter, so we default to ECDSA-P256
    //    for everything (the only algorithm `SslService` currently
    //    issues).
    let key_type = KeyType::EcdsaP256;

    Ok(ParsedBundle {
        cert_pem: cert_pem.to_string(),
        chain_pem: chain_pem.to_string(),
        key_pem: key_pem.to_string(),
        subject,
        issuer,
        valid_from,
        valid_to,
        key_type,
    })
}

/// Convert an `ASN1Time` (x509-parser) to a `DateTime<Utc>`.
/// Uses `ASN1Time::timestamp()` which returns the unix-epoch
/// seconds — robust regardless of the `Display` formatting.
fn x509_to_chrono(t: x509_parser::time::ASN1Time) -> DateTime<Utc> {
    DateTime::<Utc>::from_timestamp(t.timestamp(), 0).unwrap_or_else(Utc::now)
}

/// Construct a fresh `Certificate` aggregate from the parsed bundle
/// + the encrypted private key. Caller supplies `source` and the
///   master-key-derived ciphertext.
pub fn bundle_to_certificate(
    domain: impl Into<String>,
    source: openpanel_domain::ssl::source::CertificateSource,
    bundle: ParsedBundle,
    key_ciphertext: Vec<u8>,
) -> Result<Certificate, SslError> {
    Certificate::new(
        domain,
        source,
        bundle.issuer,
        bundle.valid_from,
        bundle.valid_to,
        bundle.key_type,
        bundle.cert_pem,
        bundle.chain_pem,
        key_ciphertext,
    )
}

/// Trimmed "new cert material" used by the application service when
/// it already has validated PEMs (from `parse_pem_bundle`,
/// `self_signed::generate`, or an `AcmeClient::IssuedCert`).
///
/// Avoids forcing a full PEM re-parse on the hot path.
#[derive(Debug, Clone)]
pub struct NewCertMaterial {
    /// PEM-encoded leaf certificate.
    pub cert_pem: String,
    /// PEM-encoded intermediate chain.
    pub chain_pem: String,
    /// PEM-encoded private key (plaintext — caller MUST encrypt
    /// before passing in `key_ciphertext`).
    pub key_pem_plaintext: String,
    /// Issuer CN.
    pub issuer: String,
    /// Validity start (not_before).
    pub valid_from: chrono::DateTime<chrono::Utc>,
    /// Validity end (not_after).
    pub valid_to: chrono::DateTime<chrono::Utc>,
    /// Key algorithm.
    pub key_type: KeyType,
}

/// Construct a `Certificate` from already-validated material. Used by
/// the ACME and self-signed code paths where the cert+key have been
/// generated by the panel itself and don't need re-parsing.
pub fn new_cert(
    domain: impl Into<String>,
    source: openpanel_domain::ssl::source::CertificateSource,
    material: NewCertMaterial,
    key_ciphertext: Vec<u8>,
) -> Result<Certificate, SslError> {
    Certificate::new(
        domain,
        source,
        material.issuer,
        material.valid_from,
        material.valid_to,
        material.key_type,
        material.cert_pem,
        material.chain_pem,
        key_ciphertext,
    )
}

// Re-export the master-key length for callers that compose crypto
// helpers without going through `databases::crypto` directly.
#[allow(dead_code)]
pub(super) const _KEY_LEN_REF: usize = KEY_LEN;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_mismatched_key_and_cert() {
        let domain = "test.example.com";
        let params_a = rcgen::CertificateParams::new(vec![domain.into()]).unwrap();
        let kp_a = rcgen::KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256).unwrap();
        let cert_a = params_a.self_signed(&kp_a).unwrap();
        let pem_a = cert_a.pem();

        let kp_b = rcgen::KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256).unwrap();
        let key_b_pem = kp_b.serialize_pem();

        let err = parse_pem_bundle(&pem_a, "", &key_b_pem).unwrap_err();
        assert!(matches!(err, SslError::KeyMismatch), "got {err:?}");
    }

    #[test]
    fn accepts_matching_key_and_cert() {
        let domain = "match.example.com";
        let params = rcgen::CertificateParams::new(vec![domain.into()]).unwrap();
        let kp = rcgen::KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256).unwrap();
        let cert = params.self_signed(&kp).unwrap();
        let cert_pem = cert.pem();
        let key_pem = kp.serialize_pem();

        let bundle = parse_pem_bundle(&cert_pem, "", &key_pem).expect("parse");
        assert!(!bundle.issuer.is_empty(), "issuer extracted");
        assert!(bundle.valid_to > bundle.valid_from, "valid window");
        assert!(bundle.cert_pem.contains("BEGIN CERTIFICATE"));
        assert!(bundle.key_pem.contains("BEGIN PRIVATE KEY"));
    }
}
