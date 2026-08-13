//! SQLite API-token repository.

use std::collections::BTreeSet;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use openpanel_domain::{ApiToken, ApiTokenRepository, Cidr, RepoError, TokenHash, TokenScope};
use sqlx::{Pool, Sqlite};
use uuid::Uuid;

/// SQLite-backed token persistence.
pub struct SqliteApiTokenRepository {
    pool: Pool<Sqlite>,
}

impl SqliteApiTokenRepository {
    /// Construct over an initialized pool.
    pub fn new(pool: Pool<Sqlite>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ApiTokenRepository for SqliteApiTokenRepository {
    async fn create(&self, token: &ApiToken) -> Result<(), RepoError> {
        insert(&self.pool, token).await
    }

    async fn update(&self, token: &ApiToken) -> Result<(), RepoError> {
        sqlx::query(
            "UPDATE api_tokens SET label=?,hash=?,scopes=?,cidr_allowlist=?,expires_at=?,last_used_at=?,revoked_at=? WHERE id=? AND user_id=?",
        )
        .bind(token.label())
        .bind(token.hash().as_bytes().as_slice())
        .bind(json_scopes(token)?)
        .bind(json_cidrs(token)?)
        .bind(token.expires_at().to_rfc3339())
        .bind(token.last_used_at().map(|value| value.to_rfc3339()))
        .bind(token.revoked_at().map(|value| value.to_rfc3339()))
        .bind(token.id().to_string())
        .bind(token.user_id().to_string())
        .execute(&self.pool)
        .await
        .map_err(repo_error)?;
        Ok(())
    }

    async fn rotate(&self, old: &ApiToken, new: &ApiToken) -> Result<(), RepoError> {
        let mut transaction = self.pool.begin().await.map_err(repo_error)?;
        sqlx::query("UPDATE api_tokens SET revoked_at=? WHERE id=? AND user_id=?")
            .bind(old.revoked_at().map(|value| value.to_rfc3339()))
            .bind(old.id().to_string())
            .bind(old.user_id().to_string())
            .execute(&mut *transaction)
            .await
            .map_err(repo_error)?;
        insert_executor(&mut *transaction, new).await?;
        transaction.commit().await.map_err(repo_error)
    }

    async fn find(&self, user_id: Uuid, id: Uuid) -> Result<Option<ApiToken>, RepoError> {
        sqlx::query_as::<_, Row>("SELECT * FROM api_tokens WHERE user_id=? AND id=?")
            .bind(user_id.to_string())
            .bind(id.to_string())
            .fetch_optional(&self.pool)
            .await
            .map_err(repo_error)?
            .map(Row::token)
            .transpose()
    }

    async fn find_by_hash(&self, hash: &TokenHash) -> Result<Option<ApiToken>, RepoError> {
        sqlx::query_as::<_, Row>("SELECT * FROM api_tokens WHERE hash=?")
            .bind(hash.as_bytes().as_slice())
            .fetch_optional(&self.pool)
            .await
            .map_err(repo_error)?
            .map(Row::token)
            .transpose()
    }

    async fn list(&self, user_id: Uuid) -> Result<Vec<ApiToken>, RepoError> {
        sqlx::query_as::<_, Row>(
            "SELECT * FROM api_tokens WHERE user_id=? ORDER BY created_at DESC",
        )
        .bind(user_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(repo_error)?
        .into_iter()
        .map(Row::token)
        .collect()
    }
}

async fn insert(pool: &Pool<Sqlite>, token: &ApiToken) -> Result<(), RepoError> {
    insert_executor(pool, token).await
}

async fn insert_executor<'e, E>(executor: E, token: &ApiToken) -> Result<(), RepoError>
where
    E: sqlx::Executor<'e, Database = Sqlite>,
{
    sqlx::query("INSERT INTO api_tokens (id,user_id,label,hash,scopes,cidr_allowlist,created_at,expires_at,last_used_at,revoked_at) VALUES (?,?,?,?,?,?,?,?,?,?)")
        .bind(token.id().to_string())
        .bind(token.user_id().to_string())
        .bind(token.label())
        .bind(token.hash().as_bytes().as_slice())
        .bind(json_scopes(token)?)
        .bind(json_cidrs(token)?)
        .bind(token.created_at().to_rfc3339())
        .bind(token.expires_at().to_rfc3339())
        .bind(token.last_used_at().map(|value| value.to_rfc3339()))
        .bind(token.revoked_at().map(|value| value.to_rfc3339()))
        .execute(executor).await.map_err(repo_error)?;
    Ok(())
}

fn json_scopes(token: &ApiToken) -> Result<String, RepoError> {
    serde_json::to_string(
        &token
            .scopes()
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
    )
    .map_err(repo_error)
}

fn json_cidrs(token: &ApiToken) -> Result<Option<String>, RepoError> {
    if token.cidr_allowlist().is_empty() {
        return Ok(None);
    }
    serde_json::to_string(
        &token
            .cidr_allowlist()
            .iter()
            .map(|cidr| cidr.as_str())
            .collect::<Vec<_>>(),
    )
    .map(Some)
    .map_err(repo_error)
}

#[derive(sqlx::FromRow)]
struct Row {
    id: String,
    user_id: String,
    label: String,
    hash: Vec<u8>,
    scopes: String,
    cidr_allowlist: Option<String>,
    created_at: String,
    expires_at: String,
    last_used_at: Option<String>,
    revoked_at: Option<String>,
}

impl Row {
    fn token(self) -> Result<ApiToken, RepoError> {
        let hash: [u8; 32] = self
            .hash
            .try_into()
            .map_err(|_| RepoError::new("invalid token hash"))?;
        let scopes = serde_json::from_str::<Vec<String>>(&self.scopes)
            .map_err(repo_error)?
            .into_iter()
            .map(|scope| TokenScope::parse(&scope).map_err(repo_error))
            .collect::<Result<BTreeSet<_>, _>>()?;
        let cidrs = self
            .cidr_allowlist
            .map(|value| serde_json::from_str::<Vec<String>>(&value).map_err(repo_error))
            .transpose()?
            .unwrap_or_default()
            .into_iter()
            .map(|cidr| Cidr::parse(&cidr).map_err(repo_error))
            .collect::<Result<Vec<_>, _>>()?;
        ApiToken::restore(
            Uuid::parse_str(&self.id).map_err(repo_error)?,
            Uuid::parse_str(&self.user_id).map_err(repo_error)?,
            self.label,
            TokenHash::from_bytes(hash),
            scopes,
            cidrs,
            parse(&self.created_at)?,
            parse(&self.expires_at)?,
            parse_opt(self.last_used_at)?,
            parse_opt(self.revoked_at)?,
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
