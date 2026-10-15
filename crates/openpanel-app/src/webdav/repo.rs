//! SQLite WebDAV account repository.

use std::path::PathBuf;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use openpanel_domain::{
    RepoError,
    webdav::{WebDavAccount, WebDavRepository},
};
use sqlx::{Pool, Sqlite};
use uuid::Uuid;

/// SQLite-backed WebDAV account repository.
pub struct SqliteWebDavRepository {
    pool: Pool<Sqlite>,
}

impl SqliteWebDavRepository {
    /// Construct over an initialized SQLite pool.
    pub fn new(pool: Pool<Sqlite>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl WebDavRepository for SqliteWebDavRepository {
    async fn create(&self, account: &WebDavAccount) -> Result<(), RepoError> {
        sqlx::query("INSERT INTO webdav_accounts (id,site_id,username,home_abs,password_hash,read_only,enabled,last_login_at,last_login_ip,created_at,disabled_at) VALUES (?,?,?,?,?,?,?,?,?,?,?)")
            .bind(account.id().to_string()).bind(account.site_id().to_string()).bind(account.username())
            .bind(account.home().to_string_lossy().as_ref()).bind(account.password_hash()).bind(account.read_only())
            .bind(account.enabled())
            .bind(account.last_login_at().map(|value| value.to_rfc3339())).bind(account.last_login_ip())
            .bind(account.created_at().to_rfc3339()).bind(account.disabled_at().map(|value| value.to_rfc3339()))
            .execute(&self.pool).await.map_err(repo_error)?;
        Ok(())
    }

    async fn update(&self, account: &WebDavAccount) -> Result<(), RepoError> {
        sqlx::query("UPDATE webdav_accounts SET password_hash=?,read_only=?,enabled=?,last_login_at=?,last_login_ip=?,disabled_at=? WHERE site_id=? AND id=?")
            .bind(account.password_hash()).bind(account.read_only()).bind(account.enabled())
            .bind(account.last_login_at().map(|value| value.to_rfc3339())).bind(account.last_login_ip())
            .bind(account.disabled_at().map(|value| value.to_rfc3339())).bind(account.site_id().to_string()).bind(account.id().to_string())
            .execute(&self.pool).await.map_err(repo_error)?;
        Ok(())
    }

    async fn delete(&self, site_id: Uuid, account_id: Uuid) -> Result<bool, RepoError> {
        Ok(
            sqlx::query("DELETE FROM webdav_accounts WHERE site_id=? AND id=?")
                .bind(site_id.to_string())
                .bind(account_id.to_string())
                .execute(&self.pool)
                .await
                .map_err(repo_error)?
                .rows_affected()
                == 1,
        )
    }

    async fn find(
        &self,
        site_id: Uuid,
        account_id: Uuid,
    ) -> Result<Option<WebDavAccount>, RepoError> {
        sqlx::query_as::<_, Row>("SELECT * FROM webdav_accounts WHERE site_id=? AND id=?")
            .bind(site_id.to_string())
            .bind(account_id.to_string())
            .fetch_optional(&self.pool)
            .await
            .map_err(repo_error)?
            .map(Row::account)
            .transpose()
    }

    async fn find_by_id(&self, account_id: Uuid) -> Result<Option<WebDavAccount>, RepoError> {
        sqlx::query_as::<_, Row>("SELECT * FROM webdav_accounts WHERE id=?")
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
    ) -> Result<Option<WebDavAccount>, RepoError> {
        sqlx::query_as::<_, Row>("SELECT * FROM webdav_accounts WHERE site_id=? AND username=?")
            .bind(site_id.to_string())
            .bind(username)
            .fetch_optional(&self.pool)
            .await
            .map_err(repo_error)?
            .map(Row::account)
            .transpose()
    }

    async fn list(&self, site_id: Uuid) -> Result<Vec<WebDavAccount>, RepoError> {
        sqlx::query_as::<_, Row>("SELECT * FROM webdav_accounts WHERE site_id=? ORDER BY username")
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
    enabled: bool,
    last_login_at: Option<String>,
    last_login_ip: Option<String>,
    created_at: String,
    disabled_at: Option<String>,
}

impl Row {
    fn account(self) -> Result<WebDavAccount, RepoError> {
        WebDavAccount::restore(
            Uuid::parse_str(&self.id).map_err(repo_error)?,
            Uuid::parse_str(&self.site_id).map_err(repo_error)?,
            self.username,
            PathBuf::from(self.home_abs),
            self.password_hash,
            self.read_only,
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
