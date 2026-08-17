//! SQLite-backed adapter for the offsite-backup-targets bounded context.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use openpanel_domain::{
    BackupCredential, CredentialKind, KekRef, OffsiteBackupError, OffsiteBackupRepository,
    RemoteTargetConfig,
};
use sqlx::{Pool, Row, Sqlite};
use uuid::Uuid;

/// SQLite-backed offsite backup repository.
#[derive(Clone)]
pub struct SqliteOffsiteBackupRepository {
    pool: Pool<Sqlite>,
}

impl SqliteOffsiteBackupRepository {
    /// Build a repo over the given pool.
    pub fn new(pool: Pool<Sqlite>) -> Self {
        Self { pool }
    }
}

fn kind_str(kind: CredentialKind) -> &'static str {
    kind.as_str()
}

fn kind_from_str(s: &str) -> CredentialKind {
    match s {
        "wasabi" => CredentialKind::Wasabi,
        "b2" => CredentialKind::B2,
        "rsync" => CredentialKind::Rsync,
        _ => CredentialKind::S3,
    }
}

fn parse_ts(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now())
}

#[async_trait]
impl OffsiteBackupRepository for SqliteOffsiteBackupRepository {
    async fn insert_credential(
        &self,
        credential: &BackupCredential,
    ) -> Result<(), OffsiteBackupError> {
        sqlx::query(
            "INSERT INTO backup_credentials (id, kind, label, secret_enc, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )
        .bind(credential.id().to_string())
        .bind(kind_str(credential.kind()))
        .bind(credential.label())
        .bind(credential.secret_enc())
        .bind(credential.created_at().to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| OffsiteBackupError::Persistence(format!("insert credential: {e}")))?;
        Ok(())
    }

    async fn find_credential(
        &self,
        id: Uuid,
    ) -> Result<Option<BackupCredential>, OffsiteBackupError> {
        let row = sqlx::query(
            "SELECT id, kind, label, secret_enc, created_at
             FROM backup_credentials WHERE id = ?1",
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| OffsiteBackupError::Persistence(format!("find credential: {e}")))?;
        let Some(row) = row else {
            return Ok(None);
        };
        let id: String = row.get("id");
        let kind: String = row.get("kind");
        let label: String = row.get("label");
        let secret_enc: String = row.get("secret_enc");
        let created_at: String = row.get("created_at");
        Ok(Some(
            BackupCredential::new(
                Uuid::parse_str(&id).map_err(|e| {
                    OffsiteBackupError::Persistence(format!("bad credential id: {e}"))
                })?,
                kind_from_str(&kind),
                label,
                secret_enc,
                parse_ts(&created_at),
            )
            .map_err(|e| OffsiteBackupError::Persistence(format!("row decode: {e}")))?,
        ))
    }

    async fn list_credentials(&self) -> Result<Vec<BackupCredential>, OffsiteBackupError> {
        let rows = sqlx::query(
            "SELECT id, kind, label, secret_enc, created_at
             FROM backup_credentials ORDER BY created_at DESC",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| OffsiteBackupError::Persistence(format!("list credentials: {e}")))?;
        rows.into_iter()
            .map(|row| {
                let id: String = row.get("id");
                let kind: String = row.get("kind");
                let label: String = row.get("label");
                let secret_enc: String = row.get("secret_enc");
                let created_at: String = row.get("created_at");
                BackupCredential::new(
                    Uuid::parse_str(&id).map_err(|e| {
                        OffsiteBackupError::Persistence(format!("bad credential id: {e}"))
                    })?,
                    kind_from_str(&kind),
                    label,
                    secret_enc,
                    parse_ts(&created_at),
                )
                .map_err(|e| OffsiteBackupError::Persistence(format!("row decode: {e}")))
            })
            .collect()
    }

    async fn delete_credential(&self, id: Uuid) -> Result<(), OffsiteBackupError> {
        let attached =
            sqlx::query("SELECT 1 FROM backup_remote_targets WHERE credential_id = ?1 LIMIT 1")
                .bind(id.to_string())
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| OffsiteBackupError::Persistence(format!("check attach: {e}")))?;
        if attached.is_some() {
            return Err(OffsiteBackupError::CredentialInUse(id));
        }
        let res = sqlx::query("DELETE FROM backup_credentials WHERE id = ?1")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| OffsiteBackupError::Persistence(format!("delete credential: {e}")))?;
        if res.rows_affected() == 0 {
            return Err(OffsiteBackupError::CredentialNotFound);
        }
        Ok(())
    }

    async fn insert_remote_target(
        &self,
        config: &RemoteTargetConfig,
    ) -> Result<(), OffsiteBackupError> {
        sqlx::query(
            "INSERT OR REPLACE INTO backup_remote_targets
             (plan_id, credential_id, prefix, schedule)
             VALUES (?1, ?2, ?3, ?4)",
        )
        .bind(config.plan_id().to_string())
        .bind(config.credential_id().to_string())
        .bind(config.prefix())
        .bind(config.schedule())
        .execute(&self.pool)
        .await
        .map_err(|e| OffsiteBackupError::Persistence(format!("insert remote target: {e}")))?;
        Ok(())
    }

    async fn remote_target_for_plan(
        &self,
        plan_id: Uuid,
    ) -> Result<Option<RemoteTargetConfig>, OffsiteBackupError> {
        let row = sqlx::query(
            "SELECT plan_id, credential_id, prefix, schedule
             FROM backup_remote_targets WHERE plan_id = ?1",
        )
        .bind(plan_id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| OffsiteBackupError::Persistence(format!("remote target: {e}")))?;
        let Some(row) = row else {
            return Ok(None);
        };
        let pid: String = row.get("plan_id");
        let cid: String = row.get("credential_id");
        let prefix: String = row.get("prefix");
        let schedule: Option<String> = row.get("schedule");
        Ok(Some(
            RemoteTargetConfig::new(
                Uuid::parse_str(&pid)
                    .map_err(|e| OffsiteBackupError::Persistence(format!("bad plan id: {e}")))?,
                Uuid::parse_str(&cid).map_err(|e| {
                    OffsiteBackupError::Persistence(format!("bad credential id: {e}"))
                })?,
                prefix,
                schedule,
            )
            .map_err(|e| OffsiteBackupError::Persistence(format!("row decode: {e}")))?,
        ))
    }

    async fn insert_kek(&self, kek: &KekRef) -> Result<(), OffsiteBackupError> {
        sqlx::query(
            "INSERT INTO backup_kek_wrappers (id, salt_hex, wrapped_kek_hex, created_at)
             VALUES (?1, ?2, ?3, ?4)",
        )
        .bind(kek.id().to_string())
        .bind(kek.salt_hex())
        .bind(kek.wrapped_kek_hex())
        .bind(kek.created_at().to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| OffsiteBackupError::Persistence(format!("insert kek: {e}")))?;
        Ok(())
    }

    async fn current_kek(&self) -> Result<Option<KekRef>, OffsiteBackupError> {
        let row = sqlx::query(
            "SELECT id, salt_hex, wrapped_kek_hex, created_at
             FROM backup_kek_wrappers ORDER BY created_at DESC LIMIT 1",
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| OffsiteBackupError::Persistence(format!("current kek: {e}")))?;
        let Some(row) = row else {
            return Ok(None);
        };
        let id: String = row.get("id");
        let salt_hex: String = row.get("salt_hex");
        let wrapped_kek_hex: String = row.get("wrapped_kek_hex");
        let created_at: String = row.get("created_at");
        Ok(Some(
            KekRef::new(
                Uuid::parse_str(&id)
                    .map_err(|e| OffsiteBackupError::Persistence(format!("bad kek id: {e}")))?,
                salt_hex,
                wrapped_kek_hex,
                parse_ts(&created_at),
            )
            .map_err(|e| OffsiteBackupError::Persistence(format!("row decode: {e}")))?,
        ))
    }
}
