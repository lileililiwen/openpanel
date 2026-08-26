//! Per-source-class logrotate rotation policies: drop-in writer,
//! persistence, drift detection, and the manual-rotate runner.

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use openpanel_core::{AuditAction, AuditEvent, AuditOutcome, AuditService};
use openpanel_domain::{
    Role, User,
    logs::{RotationPolicy, SourceClass, apply_managed_block, managed_block_markers},
};
use sqlx::SqlitePool;

use super::LogServiceError;

/// Drop-in file name for a class.
pub fn drop_in_name(class: SourceClass) -> String {
    format!("openpanel-{}", class.as_str())
}

/// v1 path registry per source class (the reader's registered
/// sources refine these globs; rotation only needs stable targets).
fn registry_paths(class: SourceClass) -> Vec<String> {
    match class {
        SourceClass::SiteAccess => vec!["/var/log/openpanel/sites/*_access.log".into()],
        SourceClass::SiteError => vec!["/var/log/openpanel/sites/*_error.log".into()],
        SourceClass::ManagedService => vec!["/var/log/nginx/error.log".into()],
        SourceClass::Panel => vec!["/var/log/openpanel/panel.log".into()],
    }
}

/// Resolve the logrotate binary from `PATH` at startup.
pub fn detect_logrotate() -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join("logrotate"))
        .find(|candidate| candidate.is_file())
}

/// Read/write rotation policies and run manual rotations.
pub struct LogRotationService {
    pool: SqlitePool,
    audit: Arc<dyn AuditService>,
    drop_in_root: PathBuf,
    logrotate_bin: Option<PathBuf>,
}

impl LogRotationService {
    /// Construct with detection of the logrotate binary.
    pub fn new(pool: SqlitePool, audit: Arc<dyn AuditService>, drop_in_root: PathBuf) -> Self {
        Self {
            pool,
            audit,
            drop_in_root,
            logrotate_bin: detect_logrotate(),
        }
    }

    /// Construct with an explicit binary override (tests inject a stub).
    pub fn with_logrotate(
        pool: SqlitePool,
        audit: Arc<dyn AuditService>,
        drop_in_root: PathBuf,
        logrotate_bin: Option<PathBuf>,
    ) -> Self {
        Self {
            pool,
            audit,
            drop_in_root,
            logrotate_bin,
        }
    }

    fn require_authorized(caller: &User) -> Result<(), LogServiceError> {
        if caller.role() == Role::Owner || caller.role() == Role::Admin {
            Ok(())
        } else {
            Err(LogServiceError::Forbidden)
        }
    }

    fn drop_in_path(&self, class: SourceClass) -> PathBuf {
        self.drop_in_root.join(drop_in_name(class))
    }

    /// Load the stored policy for a class plus its on-disk drift flag.
    pub async fn get(
        &self,
        caller: &User,
        class: SourceClass,
    ) -> Result<(RotationPolicy, bool), LogServiceError> {
        Self::require_authorized(caller)?;
        let payload: Option<String> =
            sqlx::query_scalar("SELECT payload FROM log_rotation_policies WHERE class = ?")
                .bind(class.as_str())
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| LogServiceError::Storage(e.to_string()))?;
        let policy = match payload {
            Some(json) => serde_json::from_str::<RotationPolicy>(&json)
                .map_err(|e| LogServiceError::Validation(e.to_string()))?,
            None => return Err(LogServiceError::NotFound),
        };
        let drift = self.compute_drift(&policy);
        Ok((policy, drift))
    }

    /// Persist the policy and write the drop-in atomically (managed
    /// block; foreign directives preserved).
    pub async fn set(
        &self,
        caller: &User,
        policy: RotationPolicy,
    ) -> Result<RotationPolicy, LogServiceError> {
        Self::require_authorized(caller)?;
        let stanza = policy.render_stanza(&registry_paths(policy.source_class()));
        let path = self.drop_in_path(policy.source_class());
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| LogServiceError::Storage(e.to_string()))?;
        }
        let existing = tokio::fs::read_to_string(&path).await.unwrap_or_default();
        let updated = apply_managed_block(&existing, policy.source_class(), &stanza);
        let tmp = path.with_extension("tmp");
        tokio::fs::write(&tmp, updated.as_bytes())
            .await
            .map_err(|e| LogServiceError::Storage(e.to_string()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o644))
                .map_err(|e| LogServiceError::Storage(e.to_string()))?;
        }
        tokio::fs::rename(&tmp, &path)
            .await
            .map_err(|e| LogServiceError::Storage(e.to_string()))?;

        sqlx::query(
            "INSERT INTO log_rotation_policies (class, payload) VALUES (?, ?) \
             ON CONFLICT(class) DO UPDATE SET payload = excluded.payload",
        )
        .bind(policy.source_class().as_str())
        .bind(
            serde_json::to_string(&policy)
                .map_err(|e| LogServiceError::Validation(e.to_string()))?,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| LogServiceError::Storage(e.to_string()))?;

        let _ = self
            .audit
            .record(
                AuditEvent::new(
                    caller.username().as_str(),
                    AuditAction::LogsPolicyChanged,
                    AuditOutcome::Success,
                )
                .target(policy.source_class().as_str())
                .metadata(serde_json::json!({
                    "max_age_days": policy.max_age_days(),
                    "max_size_mb": policy.max_size_mb(),
                    "keep_generations": policy.keep_generations(),
                    "compress": policy.compress(),
                })),
            )
            .await;
        Ok(policy)
    }

    /// Whether the on-disk managed block matches the stored policy.
    fn compute_drift(&self, policy: &RotationPolicy) -> bool {
        let path = self.drop_in_path(policy.source_class());
        let Ok(existing) = std::fs::read_to_string(path) else {
            return true;
        };
        let expected = policy.render_stanza(&registry_paths(policy.source_class()));
        let (begin, end) = managed_block_markers(policy.source_class());
        let start = match existing.find(&begin) {
            Some(start) => start + begin.len(),
            None => return true,
        };
        let end_pos = match existing[start..].find(&end) {
            Some(pos) => start + pos,
            None => return true,
        };
        existing[start..end_pos] != expected
    }

    /// Run `logrotate --force` on the class's drop-in with capped
    /// output. Requires a stored policy and the binary on PATH.
    pub async fn rotate(
        &self,
        caller: &User,
        class: SourceClass,
    ) -> Result<String, LogServiceError> {
        Self::require_authorized(caller)?;
        // Ensure a policy exists before forcing a rotation.
        self.get(caller, class).await?;
        let bin = self
            .logrotate_bin
            .clone()
            .ok_or_else(|| LogServiceError::Validation("logrotate_unavailable".into()))?;
        let config = self.drop_in_path(class);
        let output = tokio::process::Command::new(bin)
            .arg("--force")
            .arg(&config)
            .output()
            .await
            .map_err(|e| LogServiceError::Storage(e.to_string()))?;
        let mut combined = String::new();
        combined.push_str(&String::from_utf8_lossy(&output.stdout));
        combined.push_str(&String::from_utf8_lossy(&output.stderr));
        // Cap recorded output.
        let capped: String = combined.chars().take(512).collect();
        if !output.status.success() {
            return Err(LogServiceError::Storage(format!(
                "logrotate failed: {capped}"
            )));
        }
        let _ = self
            .audit
            .record(
                AuditEvent::new(
                    caller.username().as_str(),
                    AuditAction::LogsPolicyChanged,
                    AuditOutcome::Success,
                )
                .target(class.as_str())
                .metadata(serde_json::json!({ "rotated": true })),
            )
            .await;
        Ok(capped)
    }
}

/// Ensure the parent directory of `path` exists (used by tests).
pub fn ensure_parent(path: &Path) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
}
