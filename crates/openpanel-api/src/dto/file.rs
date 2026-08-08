use chrono::{DateTime, Utc};
use openpanel_domain::FileInfo;
use serde::{Deserialize, Serialize};

/// Public-facing description of a single file or directory entry.
#[derive(Debug, Serialize)]
pub struct FileInfoDto {
    /// Base name (no parent path).
    pub name: String,
    /// `true` if this entry is a directory.
    pub is_dir: bool,
    /// Size in bytes; for directories this is typically 0.
    pub size: u64,
    /// POSIX permission bits formatted as an octal string (e.g. `"0755"`).
    pub mode: String,
    /// Last-modified timestamp (UTC).
    pub mtime: DateTime<Utc>,
    /// Detected MIME type, when available.
    pub mime: Option<String>,
}

impl FileInfoDto {
    /// Projects a domain [`FileInfo`] into its wire DTO form.
    pub fn from_info(info: &FileInfo) -> Self {
        Self {
            name: info.name.clone(),
            is_dir: info.is_dir,
            size: info.size,
            mode: info.mode.clone(),
            mtime: info.mtime,
            mime: info.mime.clone(),
        }
    }
}

/// Response body for directory listing endpoints.
#[derive(Debug, Serialize)]
pub struct ListDirResponse {
    /// All entries (files and directories) directly inside the requested path.
    pub entries: Vec<FileInfoDto>,
}

/// Request body for a `mkdir` operation on a file route.
#[derive(Debug, Deserialize)]
pub struct MkdirRequest {
    /// When `true`, create any missing parent directories.
    #[serde(default)]
    pub recursive: bool,
}

/// Request body for renaming a file or directory.
#[derive(Debug, Deserialize)]
pub struct RenameRequest {
    /// Destination path (relative to the site chroot).
    pub to: String,
}

/// Request body for changing POSIX permissions.
#[derive(Debug, Deserialize)]
pub struct ChmodRequest {
    /// Octal mode string, optionally prefixed with a leading `0` (e.g. `"0755"`).
    pub mode: String,
}

/// Query / body for removing a file or directory.
#[derive(Debug, Deserialize)]
pub struct RemoveRequest {
    /// When `true`, remove recursively (required for non-empty directories).
    #[serde(default)]
    pub recursive: bool,
}

/// Detailed JSON body for file-related errors.
#[derive(Debug, Serialize)]
pub struct FileErrorBody {
    /// Stable machine-readable error code.
    pub error: String,
    /// Echoed back only for size-related errors (e.g. payload limits).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
}

impl FileErrorBody {
    /// Creates a new error body without an associated size.
    pub fn new(error: impl Into<String>) -> Self {
        Self {
            error: error.into(),
            size: None,
        }
    }

    /// Builder-style helper attaching a byte-size to the error body.
    pub fn with_size(mut self, size: u64) -> Self {
        self.size = Some(size);
        self
    }
}
