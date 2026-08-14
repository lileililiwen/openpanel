//! Storage layout adapter for blobs and manifests.
//!
//! The on-disk layout is one directory per namespace under
//! `storage_root`. The default implementation uses the real
//! filesystem via `tokio::fs`; tests wire a `MemoryStorageLayer`.

use async_trait::async_trait;
use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("io error: {0}")]
    Io(String),
    #[error("invalid path: {0}")]
    InvalidPath(String),
}

#[async_trait]
pub trait StorageLayer: Send + Sync {
    /// Write `bytes` at `path` (relative to `storage_root`).
    async fn write(&self, path: &str, bytes: &[u8]) -> Result<(), StorageError>;

    /// Read bytes from `path`. Returns `Ok(None)` if missing.
    async fn read(&self, path: &str) -> Result<Option<Vec<u8>>, StorageError>;

    /// Delete `path`. Returns `Ok(())` even if the path is absent.
    async fn delete(&self, path: &str) -> Result<(), StorageError>;
}

/// Filesystem-backed storage layer.
pub struct FileStorage {
    root: PathBuf,
}

impl FileStorage {
    /// Construct a new filesystem-backed storage layer.
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }
}

#[async_trait]
impl StorageLayer for FileStorage {
    async fn write(&self, path: &str, bytes: &[u8]) -> Result<(), StorageError> {
        let full = self.root.join(path);
        if let Some(parent) = full.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| StorageError::Io(e.to_string()))?;
        }
        tokio::fs::write(&full, bytes)
            .await
            .map_err(|e| StorageError::Io(e.to_string()))?;
        Ok(())
    }

    async fn read(&self, path: &str) -> Result<Option<Vec<u8>>, StorageError> {
        let full = self.root.join(path);
        match tokio::fs::read(&full).await {
            Ok(bytes) => Ok(Some(bytes)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(StorageError::Io(e.to_string())),
        }
    }

    async fn delete(&self, path: &str) -> Result<(), StorageError> {
        let full = self.root.join(path);
        match tokio::fs::remove_file(&full).await {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(StorageError::Io(e.to_string())),
        }
    }
}

/// In-memory storage layer for tests.
#[derive(Default, Clone)]
pub struct MemoryStorage {
    inner: std::sync::Arc<std::sync::Mutex<std::collections::HashMap<String, Vec<u8>>>>,
}

impl MemoryStorage {
    /// Construct a new in-memory storage layer.
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl StorageLayer for MemoryStorage {
    async fn write(&self, path: &str, bytes: &[u8]) -> Result<(), StorageError> {
        self.inner
            .lock()
            .map_err(|e| StorageError::Io(e.to_string()))?
            .insert(path.to_string(), bytes.to_vec());
        Ok(())
    }

    async fn read(&self, path: &str) -> Result<Option<Vec<u8>>, StorageError> {
        Ok(self
            .inner
            .lock()
            .map_err(|e| StorageError::Io(e.to_string()))?
            .get(path)
            .cloned())
    }

    async fn delete(&self, path: &str) -> Result<(), StorageError> {
        self.inner
            .lock()
            .map_err(|e| StorageError::Io(e.to_string()))?
            .remove(path);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn memory_storage_round_trip() {
        let storage = MemoryStorage::new();
        storage.write("ns/digest", b"hello").await.unwrap();
        let got = storage.read("ns/digest").await.unwrap();
        assert_eq!(got, Some(b"hello".to_vec()));
        storage.delete("ns/digest").await.unwrap();
        let gone = storage.read("ns/digest").await.unwrap();
        assert_eq!(gone, None);
    }
}