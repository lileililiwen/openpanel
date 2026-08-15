//! SQLite adapter for the git deployment bounded context.

use chrono::{DateTime, Utc};
use openpanel_domain::{DeployRepo, DeployRepository, DeployRun, DeployStatus, RepoError};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

/// SQLite-backed `DeployRepository`.
#[derive(Clone)]
pub struct SqliteDeployRepository {
    pool: SqlitePool,
}

impl SqliteDeployRepository {
    /// Construct a repository over the shared SQLite pool.
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl DeployRepository for SqliteDeployRepository {
    async fn save_repo(&self, repo: &DeployRepo) -> Result<(), RepoError> {
        sqlx::query(
            "INSERT OR REPLACE INTO deploy_repos \
             (id, site_id, url, branch, linked_at, build_command, docroot_subdir, webhook_secret) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(repo.id.to_string())
        .bind(repo.site_id.to_string())
        .bind(&repo.url)
        .bind(&repo.branch)
        .bind(repo.linked_at.to_rfc3339())
        .bind(&repo.build_command)
        .bind(&repo.docroot_subdir)
        .bind(&repo.webhook_secret)
        .execute(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(())
    }

    async fn get_repo(&self, site_id: Uuid) -> Result<Option<DeployRepo>, RepoError> {
        let row = sqlx::query(
            "SELECT id, site_id, url, branch, linked_at, build_command, docroot_subdir, webhook_secret \
             FROM deploy_repos WHERE site_id = ?",
        )
        .bind(site_id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        row.map(decode_repo).transpose()
    }

    async fn delete_repo(&self, site_id: Uuid) -> Result<(), RepoError> {
        sqlx::query("DELETE FROM deploy_repos WHERE site_id = ?")
            .bind(site_id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(())
    }

    async fn save_run(&self, run: &DeployRun) -> Result<(), RepoError> {
        let completed_at = run.completed_at.map(|t| t.to_rfc3339());
        sqlx::query(
            "INSERT OR REPLACE INTO deploy_runs \
             (id, repo_id, started_at, completed_at, commit_sha, status, message) \
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(run.id.to_string())
        .bind(run.repo_id.to_string())
        .bind(run.started_at.to_rfc3339())
        .bind(completed_at)
        .bind(&run.commit_sha)
        .bind(run.status.as_str())
        .bind(&run.message)
        .execute(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(())
    }

    async fn list_runs(&self, repo_id: Uuid) -> Result<Vec<DeployRun>, RepoError> {
        let rows = sqlx::query(
            "SELECT id, repo_id, started_at, completed_at, commit_sha, status, message \
             FROM deploy_runs WHERE repo_id = ? ORDER BY started_at DESC",
        )
        .bind(repo_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        rows.into_iter().map(decode_run).collect()
    }
}

fn decode_repo(row: sqlx::sqlite::SqliteRow) -> Result<DeployRepo, RepoError> {
    let id: String = row.try_get("id").map_err(map_sqlx)?;
    let site_id: String = row.try_get("site_id").map_err(map_sqlx)?;
    let url: String = row.try_get("url").map_err(map_sqlx)?;
    let branch: String = row.try_get("branch").map_err(map_sqlx)?;
    let linked_at: String = row.try_get("linked_at").map_err(map_sqlx)?;
    let build_command: String = row.try_get("build_command").map_err(map_sqlx)?;
    let docroot_subdir: String = row.try_get("docroot_subdir").map_err(map_sqlx)?;
    let webhook_secret: String = row.try_get("webhook_secret").map_err(map_sqlx)?;
    let id = Uuid::parse_str(&id).map_err(|e| RepoError::new(e.to_string()))?;
    let site_id = Uuid::parse_str(&site_id).map_err(|e| RepoError::new(e.to_string()))?;
    let linked_at = parse_ts(&linked_at)?;
    Ok(DeployRepo {
        id,
        site_id,
        url,
        branch,
        linked_at,
        build_command,
        docroot_subdir,
        webhook_secret,
    })
}

fn decode_run(row: sqlx::sqlite::SqliteRow) -> Result<DeployRun, RepoError> {
    let id: String = row.try_get("id").map_err(map_sqlx)?;
    let repo_id: String = row.try_get("repo_id").map_err(map_sqlx)?;
    let started_at: String = row.try_get("started_at").map_err(map_sqlx)?;
    let completed_at: Option<String> = row.try_get("completed_at").map_err(map_sqlx)?;
    let commit_sha: String = row.try_get("commit_sha").map_err(map_sqlx)?;
    let status: String = row.try_get("status").map_err(map_sqlx)?;
    let message: String = row.try_get("message").map_err(map_sqlx)?;
    let id = Uuid::parse_str(&id).map_err(|e| RepoError::new(e.to_string()))?;
    let repo_id = Uuid::parse_str(&repo_id).map_err(|e| RepoError::new(e.to_string()))?;
    let started_at = parse_ts(&started_at)?;
    let completed_at = completed_at.as_deref().map(parse_ts).transpose()?;
    let status = match status.as_str() {
        "pending" => DeployStatus::Pending,
        "running" => DeployStatus::Running,
        "succeeded" => DeployStatus::Succeeded,
        "failed" => DeployStatus::Failed,
        "rolled_back" => DeployStatus::RolledBack,
        other => return Err(RepoError::new(format!("unknown status: {other}"))),
    };
    Ok(DeployRun {
        id,
        repo_id,
        started_at,
        completed_at,
        commit_sha,
        status,
        message,
    })
}

fn parse_ts(s: &str) -> Result<DateTime<Utc>, RepoError> {
    DateTime::parse_from_rfc3339(s)
        .map(|t| t.with_timezone(&Utc))
        .map_err(|e| RepoError::new(format!("invalid timestamp: {e}")))
}

fn map_sqlx(e: sqlx::Error) -> RepoError {
    RepoError::new(e.to_string())
}