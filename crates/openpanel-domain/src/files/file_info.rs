use chrono::{DateTime, Utc};

/// Read model describing a file or directory entry. Returned by
/// `FilesRepository::list_dir`.
#[derive(Debug, Clone)]
pub struct FileInfo {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    pub mode: String,
    pub mtime: DateTime<Utc>,
    pub mime: Option<String>,
}

impl FileInfo {
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