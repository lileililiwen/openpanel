//! SQLite-backed collaborator and site grant repositories.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use openpanel_domain::{
    CollabStatus, Collaborator, CollaboratorId, CollaboratorRepository, Email, PermissionSet,
    SiteGrant, SiteGrantRepository, common::error::RepoError,
};
use sqlx::{Pool, Sqlite};
use uuid::Uuid;

fn parse_status(s: &str) -> Result<CollabStatus, RepoError> {
    match s {
        "invited" => Ok(CollabStatus::Invited),
        "active" => Ok(CollabStatus::Active),
        "revoked" => Ok(CollabStatus::Revoked),
        other => Err(RepoError::new(format!(
            "unknown collaborator status `{other}`"
        ))),
    }
}

fn status_str(s: CollabStatus) -> &'static str {
    match s {
        CollabStatus::Invited => "invited",
        CollabStatus::Active => "active",
        CollabStatus::Revoked => "revoked",
    }
}

fn parse_email(s: &str) -> Result<Email, RepoError> {
    Email::new(s).map_err(|e| RepoError::new(e.to_string()))
}

fn parse_dt(s: &str) -> Result<DateTime<Utc>, RepoError> {
    DateTime::parse_from_rfc3339(s)
        .map(|d| d.with_timezone(&Utc))
        .map_err(|e| RepoError::new(format!("invalid datetime `{s}`: {e}")))
}

/// SQLite-backed collaborator repository.
#[derive(Clone)]
pub struct SqliteCollaboratorRepository {
    pool: Pool<Sqlite>,
}

impl SqliteCollaboratorRepository {
    /// Construct a new repository bound to `pool`.
    pub fn new(pool: Pool<Sqlite>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl CollaboratorRepository for SqliteCollaboratorRepository {
    async fn insert(&self, c: &Collaborator) -> Result<(), RepoError> {
        sqlx::query(
            "INSERT OR REPLACE INTO collaborators
                (collaborator_id, account_id, email, status,
                 invited_at, accepted_at, revoked_at)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(c.collaborator_id.as_uuid().to_string())
        .bind(c.account_id.to_string())
        .bind(c.email.as_str())
        .bind(status_str(c.status))
        .bind(c.invited_at.to_rfc3339())
        .bind(c.accepted_at.map(|d| d.to_rfc3339()))
        .bind(c.revoked_at.map(|d| d.to_rfc3339()))
        .execute(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(())
    }

    async fn find(&self, id: &CollaboratorId) -> Result<Option<Collaborator>, RepoError> {
        let row: Option<CollaboratorRow> = sqlx::query_as(
            "SELECT collaborator_id, account_id, email, status,
                    invited_at, accepted_at, revoked_at
             FROM collaborators WHERE collaborator_id = ?",
        )
        .bind(id.as_uuid().to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        row.map(row_to_collaborator).transpose()
    }

    async fn update_status(&self, id: &CollaboratorId, c: &Collaborator) -> Result<(), RepoError> {
        let res = sqlx::query(
            "UPDATE collaborators
             SET status = ?, accepted_at = ?, revoked_at = ?
             WHERE collaborator_id = ?",
        )
        .bind(status_str(c.status))
        .bind(c.accepted_at.map(|d| d.to_rfc3339()))
        .bind(c.revoked_at.map(|d| d.to_rfc3339()))
        .bind(id.as_uuid().to_string())
        .execute(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        if res.rows_affected() == 0 {
            return Err(RepoError::new(format!("collaborator {id} not found")));
        }
        Ok(())
    }

    async fn list_for_account(&self, account_id: Uuid) -> Result<Vec<Collaborator>, RepoError> {
        let rows: Vec<CollaboratorRow> = sqlx::query_as(
            "SELECT collaborator_id, account_id, email, status,
                    invited_at, accepted_at, revoked_at
             FROM collaborators WHERE account_id = ? ORDER BY invited_at ASC",
        )
        .bind(account_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        rows.into_iter().map(row_to_collaborator).collect()
    }
}

#[derive(sqlx::FromRow)]
struct CollaboratorRow {
    collaborator_id: String,
    account_id: String,
    email: String,
    status: String,
    invited_at: String,
    accepted_at: Option<String>,
    revoked_at: Option<String>,
}

fn row_to_collaborator(row: CollaboratorRow) -> Result<Collaborator, RepoError> {
    let collaborator_id = Uuid::parse_str(&row.collaborator_id)
        .map_err(|e| RepoError::new(format!("collaborator_id: {e}")))?;
    let account_id =
        Uuid::parse_str(&row.account_id).map_err(|e| RepoError::new(format!("account_id: {e}")))?;
    let email = parse_email(&row.email)?;
    let status = parse_status(&row.status)?;
    let invited_at = parse_dt(&row.invited_at)?;
    let accepted_at = match row.accepted_at {
        Some(s) => Some(parse_dt(&s)?),
        None => None,
    };
    let revoked_at = match row.revoked_at {
        Some(s) => Some(parse_dt(&s)?),
        None => None,
    };
    Ok(Collaborator {
        collaborator_id: CollaboratorId::new(collaborator_id),
        account_id,
        email,
        status,
        invited_at,
        accepted_at,
        revoked_at,
    })
}

/// SQLite-backed site grant repository.
#[derive(Clone)]
pub struct SqliteSiteGrantRepository {
    pool: Pool<Sqlite>,
}

impl SqliteSiteGrantRepository {
    /// Construct a new repository bound to `pool`.
    pub fn new(pool: Pool<Sqlite>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl SiteGrantRepository for SqliteSiteGrantRepository {
    async fn insert(&self, g: &SiteGrant) -> Result<(), RepoError> {
        sqlx::query(
            "INSERT OR REPLACE INTO site_grants
                (collaborator_id, site_id, permissions, granted_at)
             VALUES (?, ?, ?, ?)",
        )
        .bind(g.collaborator_id.as_uuid().to_string())
        .bind(g.site_id.to_string())
        .bind(g.permissions.0 as i64)
        .bind(g.granted_at.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(())
    }

    async fn delete(
        &self,
        collaborator_id: &CollaboratorId,
        site_id: Uuid,
    ) -> Result<(), RepoError> {
        sqlx::query(
            "DELETE FROM site_grants
             WHERE collaborator_id = ? AND site_id = ?",
        )
        .bind(collaborator_id.as_uuid().to_string())
        .bind(site_id.to_string())
        .execute(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(())
    }

    async fn update_permissions(
        &self,
        collaborator_id: &CollaboratorId,
        site_id: Uuid,
        permissions: PermissionSet,
    ) -> Result<(), RepoError> {
        let res = sqlx::query(
            "UPDATE site_grants SET permissions = ?
             WHERE collaborator_id = ? AND site_id = ?",
        )
        .bind(permissions.0 as i64)
        .bind(collaborator_id.as_uuid().to_string())
        .bind(site_id.to_string())
        .execute(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        if res.rows_affected() == 0 {
            return Err(RepoError::new(format!(
                "site grant missing for collaborator {collaborator_id} on site {site_id}"
            )));
        }
        Ok(())
    }

    async fn find(
        &self,
        collaborator_id: &CollaboratorId,
        site_id: Uuid,
    ) -> Result<Option<SiteGrant>, RepoError> {
        let row: Option<SiteGrantRow> = sqlx::query_as(
            "SELECT collaborator_id, site_id, permissions, granted_at
             FROM site_grants
             WHERE collaborator_id = ? AND site_id = ?",
        )
        .bind(collaborator_id.as_uuid().to_string())
        .bind(site_id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        row.map(row_to_grant).transpose()
    }

    async fn list_for_collaborator(
        &self,
        collaborator_id: &CollaboratorId,
    ) -> Result<Vec<SiteGrant>, RepoError> {
        let rows: Vec<SiteGrantRow> = sqlx::query_as(
            "SELECT collaborator_id, site_id, permissions, granted_at
             FROM site_grants WHERE collaborator_id = ?
             ORDER BY granted_at ASC",
        )
        .bind(collaborator_id.as_uuid().to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        rows.into_iter().map(row_to_grant).collect()
    }

    async fn list_for_site(&self, site_id: Uuid) -> Result<Vec<SiteGrant>, RepoError> {
        let rows: Vec<SiteGrantRow> = sqlx::query_as(
            "SELECT collaborator_id, site_id, permissions, granted_at
             FROM site_grants WHERE site_id = ?
             ORDER BY granted_at ASC",
        )
        .bind(site_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        rows.into_iter().map(row_to_grant).collect()
    }
}

#[derive(sqlx::FromRow)]
struct SiteGrantRow {
    collaborator_id: String,
    site_id: String,
    permissions: i64,
    granted_at: String,
}

fn row_to_grant(row: SiteGrantRow) -> Result<SiteGrant, RepoError> {
    let collaborator_id = Uuid::parse_str(&row.collaborator_id)
        .map_err(|e| RepoError::new(format!("collaborator_id: {e}")))?;
    let site_id =
        Uuid::parse_str(&row.site_id).map_err(|e| RepoError::new(format!("site_id: {e}")))?;
    let granted_at = parse_dt(&row.granted_at)?;
    Ok(SiteGrant {
        collaborator_id: CollaboratorId::new(collaborator_id),
        site_id,
        permissions: PermissionSet((row.permissions as u8) & 0x0F),
        granted_at,
    })
}
