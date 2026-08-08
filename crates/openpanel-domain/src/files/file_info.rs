use chrono::{DateTime, Utc};

/// Read model describing a file or directory entry. Returned by
/// `FilesRepository::list_dir`.
#[derive(Debug, Clone)]
pub struct FileInfo {
    /// The entry's basename, relative to the listed directory.
    pub name: String,
    /// Whether the entry is a directory rather than a file.
    pub is_dir: bool,
    /// Size in bytes (`0` for directories).
    pub size: u64,
    /// Unix permission bits formatted as a string (e.g. `644`).
    pub mode: String,
    /// Last modification timestamp in UTC.
    pub mtime: DateTime<Utc>,
    /// MIME type, `None` when unknown or for directories.
    pub mime: Option<String>,
}

impl FileInfo {
    /// Create a new `FileInfo` from raw listing data.
    pub fn new(
        name: impl Into<String>,
        is_dir: bool,
        size: u64,
        mode: impl Into<String>,
        mtime: DateTime<Utc>,
        mime: Option<String>,
    ) -> Self {
        Self {
            name: name.into(),
            is_dir,
            size,
            mode: mode.into(),
            mtime,
            mime,
        }
    }
}
