//! Filesystem-backed implementation of `FileRepository`. All ops are
//! async via `tokio::fs`. The repository is stateless; per-call it
//! canonicalizes the chroot and validates every path stays inside.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path as FsPath, PathBuf};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use openpanel_domain::files::error::FileError;
use openpanel_domain::files::file_info::FileInfo;
use openpanel_domain::files::path::Path;
use openpanel_domain::files::repository::{FileRepository, MAX_READ_BYTES};

pub struct FilesystemRepository;

impl FilesystemRepository {
    pub fn new() -> Self {
        Self
    }
}

impl Default for FilesystemRepository {
    fn default() -> Self {
        Self::new()
    }
}

/// Canonicalize the chroot once at request start.
pub async fn canonicalize_chroot(chroot: &str) -> Result<PathBuf, FileError> {
    let p = std::fs::canonicalize(chroot)
        .map_err(|e| FileError::Io(format!("canonicalize chroot: {e}")))?;
    Ok(p)
}

/// Resolve a relative `Path` against the canonical chroot.
pub async fn resolve_path(
    chroot_canonical: &FsPath,
    rel: &Path,
) -> Result<PathBuf, FileError> {
    let candidate = if rel.is_root() {
        chroot_canonical.to_path_buf()
    } else {
        chroot_canonical.join(rel.as_str())
    };
    let canonical = match std::fs::canonicalize(&candidate) {
        Ok(p) => p,
        Err(_) => {
            let parent = candidate
                .parent()
                .ok_or_else(|| FileError::InvalidPath("no parent".into()))?;
            let basename = candidate
                .file_name()
                .ok_or_else(|| FileError::InvalidPath("no basename".into()))?;
            let parent_canon = std::fs::canonicalize(parent)
                .map_err(|e| FileError::Io(format!("canonicalize parent: {e}")))?;
            parent_canon.join(basename)
        }
    };
    if !canonical.starts_with(chroot_canonical) {
        return Err(FileError::PathOutsideChroot(rel.to_string()));
    }
    Ok(canonical)
}

fn mode_string(perm: std::fs::Permissions) -> String {
    format!("{:04o}", perm.mode() & 0o7777)
}

fn mtime(meta: &std::fs::Metadata) -> DateTime<Utc> {
    let m = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    DateTime::<Utc>::from_timestamp(m, 0).unwrap_or_else(Utc::now)
}

#[async_trait]
impl FileRepository for FilesystemRepository {
    async fn list_dir(
        &self,
        chroot_canonical: &FsPath,
        path: &Path,
    ) -> Result<Vec<FileInfo>, FileError> {
        let abs = resolve_path(chroot_canonical, path).await?;
        let meta = tokio::fs::metadata(&abs)
            .await
            .map_err(|e| FileError::Io(e.to_string()))?;
        if !meta.is_dir() {
            return Err(FileError::IsNotADirectory(path.to_string()));
        }
        let mut out = Vec::new();
        let mut read = tokio::fs::read_dir(&abs)
            .await
            .map_err(|e| FileError::Io(e.to_string()))?;
        while let Some(entry) = read
            .next_entry()
            .await
            .map_err(|e| FileError::Io(e.to_string()))?
        {
            let name = entry.file_name().to_string_lossy().to_string();
            let meta = entry
                .metadata()
                .await
                .map_err(|e| FileError::Io(e.to_string()))?;
            let mime = if meta.is_dir() {
                None
            } else {
                Some(mime_guess::from_path(&name).first_or_octet_stream().essence_str().to_string())
            };
            out.push(FileInfo::new(
                name,
                meta.is_dir(),
                meta.len(),
                mode_string(meta.permissions()),
                mtime(&meta),
                mime,
            ));
        }
        out.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then(a.name.cmp(&b.name)));
        Ok(out)
    }

    async fn read_file(
        &self,
        chroot_canonical: &FsPath,
        path: &Path,
    ) -> Result<(Vec<u8>, DateTime<Utc>), FileError> {
        let abs = resolve_path(chroot_canonical, path).await?;
        let meta = tokio::fs::metadata(&abs).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                FileError::NotFound(path.to_string())
            } else {
                FileError::Io(e.to_string())
            }
        })?;
        if meta.is_dir() {
            return Err(FileError::IsADirectory(path.to_string()));
        }
        if meta.len() > MAX_READ_BYTES {
            return Err(FileError::FileTooLarge(meta.len()));
        }
        let bytes = tokio::fs::read(&abs)
            .await
            .map_err(|e| FileError::Io(e.to_string()))?;
        Ok((bytes, mtime(&meta)))
    }

    async fn write_file(
        &self,
        chroot_canonical: &FsPath,
        path: &Path,
        bytes: &[u8],
    ) -> Result<(), FileError> {
        let abs = resolve_path(chroot_canonical, path).await?;
        if let Some(parent) = abs.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| FileError::Io(e.to_string()))?;
        }
        let tmp = abs.with_extension(format!(
            "{}.new",
            abs.extension()
                .and_then(|s| s.to_str())
                .unwrap_or("")
        ));
        tokio::fs::write(&tmp, bytes)
            .await
            .map_err(|e| FileError::Io(e.to_string()))?;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o644))
            .map_err(|e| FileError::Io(e.to_string()))?;
        tokio::fs::rename(&tmp, &abs)
            .await
            .map_err(|e| FileError::Io(e.to_string()))?;
        Ok(())
    }

    async fn mkdir(
        &self,
        chroot_canonical: &FsPath,
        path: &Path,
    ) -> Result<(), FileError> {
        let abs = resolve_path(chroot_canonical, path).await?;
        if abs.exists() {
            return Err(FileError::AlreadyExists(path.to_string()));
        }
        tokio::fs::create_dir_all(&abs)
            .await
            .map_err(|e| FileError::Io(e.to_string()))?;
        std::fs::set_permissions(&abs, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| FileError::Io(e.to_string()))?;
        Ok(())
    }

    async fn remove(
        &self,
        chroot_canonical: &FsPath,
        path: &Path,
        recursive: bool,
    ) -> Result<(), FileError> {
        let abs = resolve_path(chroot_canonical, path).await?;
        let meta = tokio::fs::metadata(&abs).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                FileError::NotFound(path.to_string())
            } else {
                FileError::Io(e.to_string())
            }
        })?;
        if meta.is_dir() {
            let mut read = tokio::fs::read_dir(&abs)
                .await
                .map_err(|e| FileError::Io(e.to_string()))?;
            let has_children = read
                .next_entry()
                .await
                .map_err(|e| FileError::Io(e.to_string()))?
                .is_some();
            if has_children && !recursive {
                return Err(FileError::DirectoryNotEmpty(path.to_string()));
            }
            tokio::fs::remove_dir_all(&abs)
                .await
                .map_err(|e| FileError::Io(e.to_string()))?;
        } else {
            tokio::fs::remove_file(&abs)
                .await
                .map_err(|e| FileError::Io(e.to_string()))?;
        }
        Ok(())
    }

    async fn rename(
        &self,
        chroot_canonical: &FsPath,
        from: &Path,
        to: &Path,
    ) -> Result<(), FileError> {
        let from_abs = resolve_path(chroot_canonical, from).await?;
        let to_abs = resolve_path(chroot_canonical, to).await?;
        tokio::fs::rename(&from_abs, &to_abs)
            .await
            .map_err(|e| FileError::Io(e.to_string()))?;
        Ok(())
    }

    async fn chmod(
        &self,
        chroot_canonical: &FsPath,
        path: &Path,
        mode: u32,
    ) -> Result<(), FileError> {
        let abs = resolve_path(chroot_canonical, path).await?;
        std::fs::set_permissions(&abs, std::fs::Permissions::from_mode(mode))
            .map_err(|e| FileError::Io(e.to_string()))?;
        Ok(())
    }
}