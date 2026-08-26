use thiserror::Error;

/// Errors returned by the sites bounded context.
///
/// All variants are domain errors: they describe *what* went wrong in
/// site terms, never leaking the underlying `std::io::Error` or nginx
/// exit details.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum SiteError {
    /// The primary domain does not match the hostname pattern or is
    /// out of length range.
    #[error("invalid domain: {0}")]
    InvalidDomain(String),

    /// A site with this primary domain already exists.
    #[error("duplicate domain: {0}")]
    DuplicateDomain(String),

    /// An alias is malformed, duplicates another alias, or matches the
    /// primary domain.
    #[error("invalid alias `{0}`: {1}")]
    InvalidAlias(String, String),

    /// The document root is relative or contains `..`/NUL.
    #[error("invalid document root `{0}`: must be under /var/www/")]
    InvalidDocumentRoot(String),

    /// The site does not exist.
    #[error("site not found: {0}")]
    NotFound(String),

    /// A transport-tuning policy violates its validation rules.
    #[error("invalid transport policy: {0}")]
    InvalidTransport(String),

    /// The operation is not permitted.
    #[error("forbidden")]
    Forbidden,

    /// `nginx -t` rejected the generated config; the message is the
    /// sanitized reason.
    #[error("nginx test failed: {0}")]
    NginxTest(String),

    /// `nginx -s reload` failed; the message is the sanitized reason.
    #[error("nginx reload failed: {0}")]
    NginxReload(String),

    /// The `nginx` binary is not present in PATH.
    #[error("nginx not found in PATH")]
    NginxMissing,

    /// The underlying I/O failed; the message is the sanitized reason.
    #[error("io error: {0}")]
    Io(String),

    /// A persistence error occurred; the message is the sanitized reason.
    #[error("persistence error: {0}")]
    Persistence(String),
}
