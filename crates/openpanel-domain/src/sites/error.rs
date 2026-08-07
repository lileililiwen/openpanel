use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum SiteError {
    #[error("invalid domain: {0}")]
    InvalidDomain(String),

    #[error("duplicate domain: {0}")]
    DuplicateDomain(String),

    #[error("invalid alias `{0}`: {1}")]
    InvalidAlias(String, String),

    #[error("invalid document root `{0}`: must be under /var/www/")]
    InvalidDocumentRoot(String),

    #[error("site not found: {0}")]
    NotFound(String),

    #[error("forbidden")]
    Forbidden,

    #[error("nginx test failed: {0}")]
    NginxTest(String),

    #[error("nginx reload failed: {0}")]
    NginxReload(String),

    #[error("nginx not found in PATH")]
    NginxMissing,

    #[error("io error: {0}")]
    Io(String),

    #[error("persistence error: {0}")]
    Persistence(String),
}
