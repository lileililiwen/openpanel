use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum FileError {
    #[error("path resolves outside chroot: {0}")]
    PathOutsideChroot(String),

    #[error("path is invalid: {0}")]
    InvalidPath(String),

    #[error("not found: {0}")]
    NotFound(String),

    #[error("already exists: {0}")]
    AlreadyExists(String),

    #[error("is a directory: {0}")]
    IsADirectory(String),

    #[error("is not a directory: {0}")]
    IsNotADirectory(String),

    #[error("directory not empty: {0}")]
    DirectoryNotEmpty(String),

    #[error("file too large: {0} bytes (max 50 MB)")]
    FileTooLarge(u64),

    #[error("forbidden")]
    Forbidden,

    #[error("site not found: {0}")]
    SiteNotFound(String),

    #[error("io error: {0}")]
    Io(String),
}
