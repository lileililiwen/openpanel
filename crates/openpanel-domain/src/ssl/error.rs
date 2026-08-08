//! Error type for the ssl bounded context. Pure domain — no I/O.
//!
//! `SslError::Key(...)`, `SslError::Pem(...)`, `SslError::Acme(...)`
//! carry human-readable context but MUST NEVER include private-key
//! material (plaintext or ciphertext). The encrypt-at-rest layer is
//! the caller's responsibility.

use thiserror::Error;

/// All failures surfaced by the ssl bounded context.
#[derive(Debug, Error)]
pub enum SslError {
    /// The requested domain has no certificate.
    #[error("certificate for `{0}` not found")]
    NotFound(String),

    /// The repository rejected the operation (unique violation, etc.).
    #[error("certificate repository error: {0}")]
    Repo(String),

    /// The ACME server rejected the HTTP-01 challenge (domain not
    /// reachable on :80, rate limit, etc.).
    #[error("ACME HTTP-01 challenge failed: {0}")]
    AcmeChallenge(String),

    /// A general ACME / certificate-authority error (account, JWS,
    /// directory fetch, etc.).
    #[error("ACME error: {0}")]
    Acme(String),

    /// The uploaded private key does not match the uploaded
    /// certificate's public key.
    #[error("private key does not match the supplied certificate")]
    KeyMismatch,

    /// The supplied certificate has `valid_to < now` and cannot be
    /// installed.
    #[error("certificate is already expired (valid_to in the past)")]
    Expired,

    /// A PEM block could not be parsed.
    #[error("invalid PEM: {0}")]
    InvalidPem(String),

    /// An x509 certificate could not be parsed.
    #[error("invalid x509 certificate: {0}")]
    InvalidCert(String),

    /// A private key could not be parsed.
    #[error("invalid private key: {0}")]
    InvalidKey(String),

    /// A generic I/O failure (reading / writing the cert / key files
    /// on disk).
    #[error("I/O error: {0}")]
    Io(String),

    /// Encrypting the private key at rest failed.
    #[error("failed to encrypt private key: {0}")]
    Encryption(String),

    /// Decrypting the private key from the database failed (DB
    /// corruption or master-key mismatch).
    #[error("failed to decrypt private key: {0}")]
    Decryption(String),
}
