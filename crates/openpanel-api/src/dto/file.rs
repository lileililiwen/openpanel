use chrono::{DateTime, Utc};
use openpanel_domain::FileInfo;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
pub struct FileInfoDto {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    pub mode: String,
    pub mtime: DateTime<Utc>,
    pub mime: Option<String>,
}

impl FileInfoDto {
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

#[derive(Debug, Serialize)]
pub struct ListDirResponse {
    pub entries: Vec<FileInfoDto>,
}

#[derive(Debug, Deserialize)]
pub struct MkdirRequest {
    #[serde(default)]
    pub recursive: bool,
}

#[derive(Debug, Deserialize)]
pub struct RenameRequest {
    pub to: String,
}

#[derive(Debug, Deserialize)]
pub struct ChmodRequest {
    pub mode: String,
}

#[derive(Debug, Deserialize)]
pub struct RemoveRequest {
    #[serde(default)]
    pub recursive: bool,
}

#[derive(Debug, Serialize)]
pub struct FileErrorBody {
    pub error: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
}

impl FileErrorBody {
    pub fn new(error: impl Into<String>) -> Self {
        Self {
            error: error.into(),
            size: None,
        }
    }

    pub fn with_size(mut self, size: u64) -> Self {
        self.size = Some(size);
        self
    }
}