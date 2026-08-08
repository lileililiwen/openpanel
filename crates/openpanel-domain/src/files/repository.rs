use async_trait::async_trait;
use chrono::{DateTime, Utc};

use crate::files::{error::FileError, file_info::FileInfo, path::Path};

/// The chrooted filesystem repository. Implementations are responsible
/// for path-traversal safety — the trait expresses the contract but
/// does not enforce it.
#[async_trait]
pub trait FileRepository: Send + Sync + 'static {
    /// List the entries of a directory. The `chroot_canonical` argument
    /// is the canonicalized chroot root; implementations must refuse
    /// paths that escape it.
    async fn list_dir(
        &self,
        chroot_canonical: &std::path::Path,
        path: &Path,
    ) -> Result<Vec<FileInfo>, FileError>;

    /// Returns (bytes, last_modified). Enforces the 50 MB cap; returns
    /// `FileTooLarge` if exceeded.
    async fn read_file(
        &self,
        chroot_canonical: &std::path::Path,
        path: &Path,
    ) -> Result<(Vec<u8>, DateTime<Utc>), FileError>;

    /// Write `bytes` to `path`, replacing any existing file.
    async fn write_file(
        &self,
        chroot_canonical: &std::path::Path,
        path: &Path,
        bytes: &[u8],
    ) -> Result<(), FileError>;

    /// Create a directory at `path`. Returns `AlreadyExists` if one is
    /// already there.
    async fn mkdir(&self, chroot_canonical: &std::path::Path, path: &Path)
    -> Result<(), FileError>;

    /// Remove a file, or a directory tree when `recursive` is set.
    async fn remove(
        &self,
        chroot_canonical: &std::path::Path,
        path: &Path,
        recursive: bool,
    ) -> Result<(), FileError>;

    /// Rename (move) `from` to `to` within the same chroot.
    async fn rename(
        &self,
        chroot_canonical: &std::path::Path,
        from: &Path,
        to: &Path,
    ) -> Result<(), FileError>;

    /// Change the Unix permission bits of `path` to `mode`.
    async fn chmod(
        &self,
        chroot_canonical: &std::path::Path,
        path: &Path,
        mode: u32,
    ) -> Result<(), FileError>;
}

/// Upper size limit for files read through [`FileRepository`].
pub const MAX_READ_BYTES: u64 = 50 * 1024 * 1024;
