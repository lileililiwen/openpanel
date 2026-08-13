//! Canonicalize-once FTP site-root storage guard.

use std::path::{Path, PathBuf};

use openpanel_domain::ftp::{FtpError, FtpPath};

/// Resolves FTP paths while denying traversal and symlink escapes.
#[derive(Debug, Clone)]
pub struct ChrootStorage {
    root: PathBuf,
}

impl ChrootStorage {
    /// Canonicalize and retain the site root once.
    pub fn new(root: impl AsRef<Path>) -> Result<Self, FtpError> {
        let root = std::fs::canonicalize(root).map_err(internal)?;
        Ok(Self { root })
    }

    /// Resolve an existing path and prove it remains under the retained root.
    pub fn resolve_existing(&self, path: &FtpPath) -> Result<PathBuf, FtpError> {
        let candidate = std::fs::canonicalize(self.root.join(path.as_path())).map_err(internal)?;
        self.ensure_inside(candidate)
    }

    /// Resolve a new leaf by canonicalizing its existing parent.
    pub fn resolve_new(&self, path: &FtpPath) -> Result<PathBuf, FtpError> {
        let joined = self.root.join(path.as_path());
        let name = joined
            .file_name()
            .ok_or_else(|| FtpError::Invalid("missing file name".into()))?;
        let parent = joined.parent().ok_or(FtpError::OperationDenied)?;
        let parent = std::fs::canonicalize(parent).map_err(internal)?;
        if !parent.starts_with(&self.root) {
            return Err(FtpError::OperationDenied);
        }
        Ok(parent.join(name))
    }

    fn ensure_inside(&self, candidate: PathBuf) -> Result<PathBuf, FtpError> {
        if candidate.starts_with(&self.root) {
            Ok(candidate)
        } else {
            Err(FtpError::OperationDenied)
        }
    }
}

fn internal(error: impl std::fmt::Display) -> FtpError {
    FtpError::Internal(error.to_string())
}
