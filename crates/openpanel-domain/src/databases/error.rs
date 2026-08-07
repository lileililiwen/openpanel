use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum DatabaseError {
    #[error("invalid database name: {0}")]
    InvalidName(String),

    #[error("invalid charset: {0}")]
    InvalidCharset(String),

    #[error("duplicate database: {0}")]
    DuplicateDatabase(String),

    #[error("database not found: {0}")]
    NotFound(String),

    #[error("forbidden")]
    Forbidden,

    #[error("mysql CLI not found in PATH")]
    MysqlMissing,

    #[error("mysql error: {0}")]
    MysqlError(String),

    #[error("master key missing — set OPENPANEL__DATABASE__MASTER_KEY")]
    MasterKeyMissing,

    #[error("encryption error: {0}")]
    Encryption(String),

    #[error("decryption error: {0}")]
    Decryption(String),

    #[error("persistence error: {0}")]
    Persistence(String),

    #[error("io error: {0}")]
    Io(String),
}
