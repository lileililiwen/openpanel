//! SQLite FTP account repository.

use std::path::PathBuf;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use openpanel_domain::{
    RepoError,
    ftp::{FtpAccount, FtpLimits, FtpRepository},
};
use sqlx::{Pool, Sqlite};
use uuid::Uuid;

/// SQLite-backed FTP account repository.
pub struct SqliteFtpRepository {
    pool: Pool<Sqlite>,
}

impl SqliteFtpRepository {
    /// Construct over an initialized SQLite pool.
    pub fn new(pool: Pool<Sqlite>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl FtpRepository for SqliteFtpRepository {
    async fn create(&self, account: &FtpAccount) -> Result<(), RepoError> {
        sqlx::query("INSERT INTO ftp_accounts (id,site_id,username,home_abs,password_hash,read_only,bandwidth_kb_per_session,max_concurrent_connections,enabled,last_login_at,last_login_ip,created_at,disabled_at) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?)")
            .bind(account.id().to_string()).bind(account.site_id().to_string()).bind(account.username())
            .bind(account.home().to_string_lossy().as_ref()).bind(account.password_hash()).bind(account.read_only())
            .bind(i64::try_from(account.limits().bandwidth_kb_per_session()).map_err(repo_error)?)
            .bind(i64::from(account.limits().max_concurrent_connections())).bind(account.enabled())
            .bind(account.last_login_at().map(|value| value.to_rfc3339())).bind(account.last_login_ip())
            .bind(account.created_at().to_rfc3339()).bind(account.disabled_at().map(|value| value.to_rfc3339()))
            .execute(&self.pool).await.map_err(repo_error)?;
        Ok(())
    }

    async fn update(&self, account: &FtpAccount) -> Result<(), RepoError> {
        sqlx::query("UPDATE ftp_accounts SET password_hash=?,read_only=?,bandwidth_kb_per_session=?,max_concurrent_connections=?,enabled=?,last_login_at=?,last_login_ip=?,disabled_at=? WHERE site_id=? AND id=?")
            .bind(account.password_hash()).bind(account.read_only())
            .bind(i64::try_from(account.limits().bandwidth_kb_per_session()).map_err(repo_error)?)
            .bind(i64::from(account.limits().max_concurrent_connections())).bind(account.enabled())
            .bind(account.last_login_at().map(|value| value.to_rfc3339())).bind(account.last_login_ip())
            .bind(account.disabled_at().map(|value| value.to_rfc3339())).bind(account.site_id().to_string()).bind(account.id().to_string())
            .execute(&self.pool).await.map_err(repo_error)?;
        Ok(())
    }

    async fn delete(&self, site_id: Uuid, account_id: Uuid) -> Result<bool, RepoError> {
        Ok(
            sqlx::query("DELETE FROM ftp_accounts WHERE site_id=? AND id=?")
                .bind(site_id.to_string())
                .bind(account_id.to_string())
                .execute(&self.pool)
                .await
                .map_err(repo_error)?
                .rows_affected()
                == 1,
        )
    }

    async fn find(&self, site_id: Uuid, account_id: Uuid) -> Result<Option<FtpAccount>, RepoError> {
        sqlx::query_as::<_, Row>("SELECT * FROM ftp_accounts WHERE site_id=? AND id=?")
            .bind(site_id.to_string())
            .bind(account_id.to_string())
            .fetch_optional(&self.pool)
            .await
            .map_err(repo_error)?
            .map(Row::account)
            .transpose()
    }

    async fn find_by_username(
        &self,
        site_id: Uuid,
        username: &str,
    ) -> Result<Option<FtpAccount>, RepoError> {
        sqlx::query_as::<_, Row>("SELECT * FROM ftp_accounts WHERE site_id=? AND username=?")
            .bind(site_id.to_string())
            .bind(username)
            .fetch_optional(&self.pool)
            .await
            .map_err(repo_error)?
            .map(Row::account)
            .transpose()
    }

    async fn list(&self, site_id: Uuid) -> Result<Vec<FtpAccount>, RepoError> {
        sqlx::query_as::<_, Row>("SELECT * FROM ftp_accounts WHERE site_id=? ORDER BY username")
            .bind(site_id.to_string())
            .fetch_all(&self.pool)
            .await
            .map_err(repo_error)?
            .into_iter()
            .map(Row::account)
            .collect()
    }
}

#[derive(sqlx::FromRow)]
struct Row {
    id: String,
    site_id: String,
    username: String,
    home_abs: String,
    password_hash: String,
    read_only: bool,
    bandwidth_kb_per_session: i64,
    max_concurrent_connections: i64,
    enabled: bool,
    last_login_at: Option<String>,
    last_login_ip: Option<String>,
    created_at: String,
    disabled_at: Option<String>,
}

impl Row {
    fn account(self) -> Result<FtpAccount, RepoError> {
        FtpAccount::restore(
            Uuid::parse_str(&self.id).map_err(repo_error)?,
            Uuid::parse_str(&self.site_id).map_err(repo_error)?,
            self.username,
            PathBuf::from(self.home_abs),
            self.password_hash,
            self.read_only,
            FtpLimits::new(
                u64::try_from(self.bandwidth_kb_per_session).map_err(repo_error)?,
                u16::try_from(self.max_concurrent_connections).map_err(repo_error)?,
            )
            .map_err(repo_error)?,
            self.enabled,
            parse_opt(self.last_login_at)?,
            self.last_login_ip,
            parse(&self.created_at)?,
            parse_opt(self.disabled_at)?,
        )
        .map_err(repo_error)
    }
}

fn parse(value: &str) -> Result<DateTime<Utc>, RepoError> {
    Ok(DateTime::parse_from_rfc3339(value)
        .map_err(repo_error)?
        .with_timezone(&Utc))
}
fn parse_opt(value: Option<String>) -> Result<Option<DateTime<Utc>>, RepoError> {
    value.map(|item| parse(&item)).transpose()
}
fn repo_error(error: impl std::fmt::Display) -> RepoError {
    RepoError::new(error.to_string())
}
