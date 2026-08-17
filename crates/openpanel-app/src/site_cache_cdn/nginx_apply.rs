//! Atomic nginx include-snippet application for a site's cache
//! policy. Writes the snippet produced by `cache_directives(...)` to
//! the site's panel-owned include path, then validates the full
//! config with `nginx -t` and reloads. On a failed `nginx -t`, the
//! prior snippet is restored atomically.
//!
//! When nginx is not installed (tests, dev sandboxes) the write is
//! still performed but `-t` / reload are skipped — a successful no-op.

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

/// Outcome of an apply attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplyOutcome {
    /// Snippet written; `nginx -t` passed; reload issued.
    Applied,
    /// nginx binary absent — snippet written, no validation/reload.
    NoNginx,
}

/// Errors that can occur while applying a cache snippet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NginxApplyError {
    /// Filesystem failure during write/restore.
    Io(String),
    /// `nginx -t` returned a non-zero status (prior snippet restored).
    TestFailed(String),
    /// `nginx -s reload` failed after a passing `-t`.
    ReloadFailed(String),
}

/// Applies per-site cache snippets under a panel-owned include root.
#[derive(Clone)]
pub struct NginxCacheManager {
    include_root: PathBuf,
    nginx_binary: PathBuf,
}

impl NginxCacheManager {
    /// Build a manager rooted at `include_root`. The nginx binary is
    /// detected via `which`, falling back to `/usr/sbin/nginx`.
    pub fn new(include_root: impl Into<PathBuf>) -> Self {
        let mut binary = PathBuf::from("/usr/sbin/nginx");
        if let Ok(out) = Command::new("which").arg("nginx").output()
            && out.status.success()
            && let Ok(s) = String::from_utf8(out.stdout)
        {
            let s = s.trim();
            if !s.is_empty() {
                binary = PathBuf::from(s);
            }
        }
        Self {
            include_root: include_root.into(),
            nginx_binary: binary,
        }
    }

    /// Override the nginx binary path (used by tests with a fake).
    pub fn with_nginx_binary(mut self, binary: impl Into<PathBuf>) -> Self {
        self.nginx_binary = binary.into();
        self
    }

    fn include_path(&self, site_id: uuid::Uuid) -> PathBuf {
        self.include_root
            .join(format!("{}.cache.conf", site_id.simple()))
    }

    /// True when the configured nginx binary exists on disk.
    pub fn nginx_available(&self) -> bool {
        self.nginx_binary.exists()
    }

    /// Write `snippet` for `site_id` and validate/reload.
    pub fn apply(
        &self,
        site_id: uuid::Uuid,
        snippet: &str,
    ) -> Result<ApplyOutcome, NginxApplyError> {
        let target = self.include_path(site_id);
        fs::create_dir_all(&self.include_root)
            .map_err(|e| NginxApplyError::Io(format!("mkdir: {e}")))?;

        let previous = if target.exists() {
            Some(
                fs::read_to_string(&target)
                    .map_err(|e| NginxApplyError::Io(format!("read: {e}")))?,
            )
        } else {
            None
        };

        // Atomic-ish write: render to `.new`, then rename into place.
        let tmp = self
            .include_root
            .join(format!("{}.cache.conf.new", site_id.simple()));
        fs::write(&tmp, snippet).map_err(|e| NginxApplyError::Io(format!("write tmp: {e}")))?;

        if !self.nginx_available() {
            // No nginx: keep the snippet, skip validation/reload.
            fs::rename(&tmp, &target).map_err(|e| NginxApplyError::Io(format!("rename: {e}")))?;
            return Ok(ApplyOutcome::NoNginx);
        }

        // Swap in the new snippet, keeping `previous` for rollback.
        fs::rename(&tmp, &target).map_err(|e| NginxApplyError::Io(format!("rename: {e}")))?;

        if !self.test()? {
            // Restore the prior snippet (or remove if none) and fail.
            if let Some(prev) = previous {
                fs::write(&target, prev)
                    .map_err(|e| NginxApplyError::Io(format!("restore: {e}")))?;
            } else if target.exists() {
                let _ = fs::remove_file(&target);
            }
            return Err(NginxApplyError::TestFailed(format!(
                "nginx -t failed for site {site_id}"
            )));
        }

        self.reload()?;
        Ok(ApplyOutcome::Applied)
    }

    fn test(&self) -> Result<bool, NginxApplyError> {
        let out = Command::new(&self.nginx_binary)
            .arg("-t")
            .output()
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    NginxApplyError::Io("nginx binary not found".to_string())
                } else {
                    NginxApplyError::Io(format!("nginx -t spawn: {e}"))
                }
            })?;
        Ok(out.status.success())
    }

    fn reload(&self) -> Result<(), NginxApplyError> {
        let out = Command::new(&self.nginx_binary)
            .arg("-s")
            .arg("reload")
            .output()
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    NginxApplyError::Io("nginx binary not found".to_string())
                } else {
                    NginxApplyError::Io(format!("nginx -s reload spawn: {e}"))
                }
            })?;
        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
            return Err(NginxApplyError::ReloadFailed(stderr));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    #[test]
    fn no_nginx_is_a_successful_noop() {
        let root = std::env::temp_dir().join(format!("op-cache-test-{}", uuid::Uuid::new_v4()));
        let mgr = NginxCacheManager::new(root.clone()).with_nginx_binary("/nonexistent/nginx");
        let site = uuid::Uuid::new_v4();
        let outcome = mgr.apply(site, "proxy_cache z;").expect("apply");
        assert_eq!(outcome, ApplyOutcome::NoNginx);
        let written = fs::read_to_string(mgr.include_path(site)).expect("file written");
        assert_eq!(written, "proxy_cache z;");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn failed_nginx_test_rolls_back() {
        // A fake nginx that reports failure on `nginx -t`.
        let dir = std::env::temp_dir().join(format!("op-cache-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let fake = dir.join("fake-nginx");
        let mut f = fs::File::create(&fake).unwrap();
        writeln!(f, "#!/bin/sh").unwrap();
        // `-t` fails, `-s reload` would succeed (never reached).
        writeln!(f, "if [ \"$1\" = \"-t\" ]; then exit 1; fi").unwrap();
        writeln!(f, "exit 0").unwrap();
        drop(f);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&fake).unwrap().permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&fake, perms).unwrap();
        }
        let root = dir.join("includes");
        let mgr = NginxCacheManager::new(root.clone()).with_nginx_binary(fake.clone());
        let site = uuid::Uuid::new_v4();

        // Seed a prior snippet so rollback restores it.
        fs::create_dir_all(&root).unwrap();
        fs::write(mgr.include_path(site), "previous snippet").unwrap();

        let err = mgr.apply(site, "new snippet").expect_err("must fail");
        assert!(matches!(err, NginxApplyError::TestFailed(_)));
        // Prior snippet restored verbatim.
        let restored = fs::read_to_string(mgr.include_path(site)).unwrap();
        assert_eq!(restored, "previous snippet");

        let _ = fs::remove_dir_all(&dir);
    }
}
