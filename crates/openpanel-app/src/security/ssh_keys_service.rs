//! Admin SSH host-key CRUD: SQLite persistence, atomic
//! `authorized_keys` writer (0600 + post-stat assert), audit, and the
//! last-used matcher input.

use std::{path::PathBuf, sync::Arc};

use chrono::Utc;
use openpanel_core::{AuditAction, AuditEvent, AuditOutcome, AuditService};
use openpanel_domain::{
    Role, User,
    security::{HostSshKey, KeyAlgo, SecurityError, render_authorized_keys},
};
use sqlx::SqlitePool;
use uuid::Uuid;

/// Admin SSH host-key registry and `authorized_keys` writer.
pub struct HostSshKeysService {
    pool: SqlitePool,
    audit: Arc<dyn AuditService>,
    authorized_keys_path: PathBuf,
}

impl HostSshKeysService {
    /// Construct over the shared pool with an explicit
    /// `authorized_keys` path.
    pub fn new(
        pool: SqlitePool,
        audit: Arc<dyn AuditService>,
        authorized_keys_path: PathBuf,
    ) -> Self {
        Self {
            pool,
            audit,
            authorized_keys_path,
        }
    }

    fn require_admin(caller: &User) -> Result<(), SecurityError> {
        if caller.role() == Role::Admin || caller.role() == Role::Owner {
            Ok(())
        } else {
            Err(SecurityError::Forbidden)
        }
    }

    /// List all registered keys.
    pub async fn list(&self, caller: &User) -> Result<Vec<HostSshKey>, SecurityError> {
        Self::require_admin(caller)?;
        let rows = sqlx::query_as::<
            _,
            (
                String,
                String,
                String,
                String,
                String,
                String,
                String,
                Option<String>,
            ),
        >(
            "SELECT id, label, fingerprint, public_key_b64, algo, added_by, added_at, last_used_at \
             FROM host_ssh_keys ORDER BY added_at",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| SecurityError::Persistence(e.to_string()))?;
        rows.into_iter()
            .map(|(id, label, fp, b64, algo, by, at, used)| {
                Ok(HostSshKey::restore(
                    Uuid::parse_str(&id).map_err(|e| SecurityError::Persistence(e.to_string()))?,
                    label,
                    fp,
                    b64,
                    parse_algo(&algo)?,
                    Uuid::parse_str(&by).map_err(|e| SecurityError::Persistence(e.to_string()))?,
                    parse_time(&at)?,
                    match used {
                        Some(t) => Some(parse_time(&t)?),
                        None => None,
                    },
                ))
            })
            .collect()
    }

    /// Parse, deduplicate, persist, and rewrite `authorized_keys`.
    pub async fn add(
        &self,
        caller: &User,
        label: String,
        line: &str,
    ) -> Result<HostSshKey, SecurityError> {
        Self::require_admin(caller)?;
        let key = HostSshKey::parse(label, line, caller.id(), Utc::now())?;
        let dup: Option<String> =
            sqlx::query_scalar("SELECT fingerprint FROM host_ssh_keys WHERE fingerprint = ?")
                .bind(key.fingerprint())
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| SecurityError::Persistence(e.to_string()))?;
        if dup.is_some() {
            return Err(SecurityError::DuplicateSshKey);
        }
        sqlx::query(
            "INSERT INTO host_ssh_keys (id, label, fingerprint, public_key_b64, algo, added_by, added_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(key.id().to_string())
        .bind(key.label())
        .bind(key.fingerprint())
        .bind(key.public_key_b64())
        .bind(key.algo().as_str())
        .bind(caller.id().to_string())
        .bind(key.added_at().to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| SecurityError::Persistence(e.to_string()))?;
        self.rewrite_file(&caller.username().to_string()).await?;
        Ok(key)
    }

    /// Remove a key by id and rewrite `authorized_keys`.
    pub async fn remove(&self, caller: &User, id: Uuid) -> Result<(), SecurityError> {
        Self::require_admin(caller)?;
        let result = sqlx::query("DELETE FROM host_ssh_keys WHERE id = ?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| SecurityError::Persistence(e.to_string()))?;
        if result.rows_affected() == 0 {
            return Err(SecurityError::NotFound(id.to_string()));
        }
        self.rewrite_file(&caller.username().to_string()).await?;
        Ok(())
    }

    /// Feed one parsed auth-log line (`SHA256:<fp>` match) into the
    /// last-used column.
    pub async fn record_last_used(
        &self,
        fingerprint: &str,
        at: chrono::DateTime<chrono::Utc>,
    ) -> Result<(), SecurityError> {
        sqlx::query("UPDATE host_ssh_keys SET last_used_at = ? WHERE fingerprint = ?")
            .bind(at.to_rfc3339())
            .bind(fingerprint)
            .execute(&self.pool)
            .await
            .map_err(|e| SecurityError::Persistence(e.to_string()))?;
        Ok(())
    }

    async fn rewrite_file(&self, actor: &str) -> Result<(), SecurityError> {
        let keys = self.list_unchecked().await?;
        let existing = std::fs::read_to_string(&self.authorized_keys_path).unwrap_or_default();
        let rendered = render_authorized_keys(&existing, &keys);
        let tmp = self.authorized_keys_path.with_extension("tmp");
        if let Some(parent) = self.authorized_keys_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| SecurityError::Persistence(e.to_string()))?;
        }
        std::fs::write(&tmp, rendered.as_bytes())
            .map_err(|e| SecurityError::Persistence(e.to_string()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))
                .map_err(|e| SecurityError::Persistence(e.to_string()))?;
        }
        std::fs::rename(&tmp, &self.authorized_keys_path)
            .map_err(|e| SecurityError::Persistence(e.to_string()))?;
        // Post-rename stat assert: mode must be 0600.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let meta = std::fs::metadata(&self.authorized_keys_path)
                .map_err(|e| SecurityError::Persistence(e.to_string()))?;
            debug_assert_eq!(meta.permissions().mode() & 0o777, 0o600);
        }
        let _ = self
            .audit
            .record(
                AuditEvent::new(actor, AuditAction::SshKeyChanged, AuditOutcome::Success)
                    .target(self.authorized_keys_path.display().to_string()),
            )
            .await;
        Ok(())
    }

    async fn list_unchecked(&self) -> Result<Vec<HostSshKey>, SecurityError> {
        let rows = sqlx::query_as::<_, (String, String, String, String, String)>(
            "SELECT id, label, fingerprint, public_key_b64, algo FROM host_ssh_keys ORDER BY added_at",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| SecurityError::Persistence(e.to_string()))?;
        rows.into_iter()
            .map(|(id, label, fp, b64, algo)| {
                Ok(HostSshKey::restore(
                    Uuid::parse_str(&id).map_err(|e| SecurityError::Persistence(e.to_string()))?,
                    label,
                    fp,
                    b64,
                    parse_algo(&algo)?,
                    Uuid::nil(),
                    Utc::now(),
                    None,
                ))
            })
            .collect()
    }
}

fn parse_algo(name: &str) -> Result<KeyAlgo, SecurityError> {
    KeyAlgo::parse(name).ok_or_else(|| SecurityError::InvalidSshKey("unknown algo".into()))
}

fn parse_time(raw: &str) -> Result<chrono::DateTime<chrono::Utc>, SecurityError> {
    chrono::DateTime::parse_from_rfc3339(raw)
        .map(|t| t.with_timezone(&chrono::Utc))
        .map_err(|e| SecurityError::Persistence(e.to_string()))
}
