use async_trait::async_trait;
use chrono::{DateTime, Utc};

use crate::files::error::FileError;
use crate::files::file_info::FileInfo;
use crate::files::path::Path;

/// The chrooted filesystem repository. Implementations are responsible
/// for path-traversal safety — the trait expresses the contract but
/// does not enforce it.
#[async_trait]
pub trait FileRepository: Send + Sync + 'static {
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

    async fn write_file(
        &self,
        chroot_canonical: &std::path::Path,
        path: &Path,
        bytes: &[u8],
    ) -> Result<(), FileError>;

    async fn mkdir(
        &self,
        chroot_canonical: &std::path::Path,
        path: &Path,
    ) -> Result<(), FileError>;

    async fn remove(
        &self,
        chroot_canonical: &std::path::Path,
        path: &Path,
        recursive: bool,
    ) -> Result<(), FileError>;

    async fn rename(
        &self,
        chroot_canonical: &std::path::Path,
        from: &Path,
        to: &Path,
    ) -> Result<(), FileError>;

    async fn chmod(
        &self,
        chroot_canonical: &std::path::Path,
        path: &Path,
        mode: u32,
    ) -> Result<(), FileError>;
}

pub const MAX_READ_BYTES: u64 = 50 * 1024 * 1024;