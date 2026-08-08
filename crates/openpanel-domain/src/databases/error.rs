use thiserror::Error;

/// Errors returned by the databases bounded context.
///
/// Variants describe what went wrong in domain terms; messages are
/// sanitized and never leak credentials or full CLI output.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum DatabaseError {
    /// The database name or owner username failed validation.
    #[error("invalid database name: {0}")]
    InvalidName(String),

    /// The requested charset is not supported.
    #[error("invalid charset: {0}")]
    InvalidCharset(String),

    /// A database with this name already exists.
    #[error("duplicate database: {0}")]
    DuplicateDatabase(String),

    /// The requested database does not exist.
    #[error("database not found: {0}")]
    NotFound(String),

    /// The operation is not permitted.
    #[error("forbidden")]
    Forbidden,

    /// The `mysql` CLI binary was not found in PATH.
    #[error("mysql CLI not found in PATH")]
    MysqlMissing,

    /// The `mysql` CLI reported an error; the message is the sanitized reason.
    #[error("mysql error: {0}")]
    MysqlError(String),

    /// The encryption master key is not configured.
    #[error("master key missing — set OPENPANEL__DATABASE__MASTER_KEY")]
    MasterKeyMissing,

    /// Encryption failed; the message is the sanitized reason.
    #[error("encryption error: {0}")]
    Encryption(String),

    /// Decryption failed; the message is the sanitized reason.
    #[error("decryption error: {0}")]
    Decryption(String),

    /// Persisting the database failed.
    #[error("persistence error: {0}")]
    Persistence(String),

    /// The underlying I/O failed; the message is the sanitized reason.
    #[error("io error: {0}")]
    Io(String),
}
