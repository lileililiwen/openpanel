use thiserror::Error;

/// Errors returned by the files bounded context.
///
/// All variants are domain errors: they describe *what* went wrong in
/// file-manager terms, never leaking the underlying `std::io::Error`.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum FileError {
    /// The requested path resolved outside the site's chroot.
    #[error("path resolves outside chroot: {0}")]
    PathOutsideChroot(String),

    /// The path violates one of `Path`'s invariants (absolute, `..`,
    /// or null byte).
    #[error("path is invalid: {0}")]
    InvalidPath(String),

    /// The file or directory does not exist.
    #[error("not found: {0}")]
    NotFound(String),

    /// A file or directory with this name already exists.
    #[error("already exists: {0}")]
    AlreadyExists(String),

    /// The operation requires a file but the path is a directory.
    #[error("is a directory: {0}")]
    IsADirectory(String),

    /// The operation requires a directory but the path is a file.
    #[error("is not a directory: {0}")]
    IsNotADirectory(String),

    /// The directory is not empty and cannot be removed.
    #[error("directory not empty: {0}")]
    DirectoryNotEmpty(String),

    /// The file exceeds the 50 MB cap.
    #[error("file too large: {0} bytes (max 50 MB)")]
    FileTooLarge(u64),

    /// The operation is not permitted.
    #[error("forbidden")]
    Forbidden,

    /// The site the operation targets does not exist.
    #[error("site not found: {0}")]
    SiteNotFound(String),

    /// The underlying I/O failed; the message is the sanitized reason.
    #[error("io error: {0}")]
    Io(String),
}
