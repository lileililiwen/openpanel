//! SQLite adapter for the database privilege bounded context.

use chrono::{DateTime, Utc};
use openpanel_domain::{
    AdminToolSession, DbGrant, DbPrivilegeRepository, GrantScope, Privilege, RemoteAccess,
    RepoError,
};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

/// SQLite-backed `DbPrivilegeRepository`.
#[derive(Clone)]
pub struct SqliteDbPrivilegeRepository {
    pool: SqlitePool,
}

impl SqliteDbPrivilegeRepository {
    /// Construct a repository over the shared SQLite pool.
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl DbPrivilegeRepository for SqliteDbPrivilegeRepository {
    async fn save_grant(&self, grant: &DbGrant) -> Result<(), RepoError> {
        let (kind, name) = match &grant.scope {
            GrantScope::Database => ("database", None),
            GrantScope::Table { name } => ("table", Some(name.clone())),
            GrantScope::Routine { name } => ("routine", Some(name.clone())),
        };
        sqlx::query(
            "INSERT OR REPLACE INTO db_grants \
             (id, database_id, user_id, scope_kind, scope_name, privilege, granted_by, granted_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(grant.id.to_string())
        .bind(grant.database_id.to_string())
        .bind(grant.user_id.to_string())
        .bind(kind)
        .bind(name)
        .bind(grant.privilege.as_str())
        .bind(grant.granted_by.to_string())
        .bind(grant.granted_at.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(())
    }

    async fn list_grants(&self, database_id: Uuid) -> Result<Vec<DbGrant>, RepoError> {
        let rows = sqlx::query(
            "SELECT id, database_id, user_id, scope_kind, scope_name, privilege, granted_by, granted_at \
             FROM db_grants WHERE database_id = ?",
        )
        .bind(database_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        rows.into_iter().map(decode_grant).collect()
    }

    async fn list_user_grants(
        &self,
        database_id: Uuid,
        user_id: Uuid,
    ) -> Result<Vec<DbGrant>, RepoError> {
        let rows = sqlx::query(
            "SELECT id, database_id, user_id, scope_kind, scope_name, privilege, granted_by, granted_at \
             FROM db_grants WHERE database_id = ? AND user_id = ?",
        )
        .bind(database_id.to_string())
        .bind(user_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        rows.into_iter().map(decode_grant).collect()
    }

    async fn delete_grant(&self, id: Uuid) -> Result<(), RepoError> {
        sqlx::query("DELETE FROM db_grants WHERE id = ?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(())
    }

    async fn save_remote_access(&self, access: &RemoteAccess) -> Result<(), RepoError> {
        let cidrs = serde_json::to_string(&access.allow_cidrs)
            .map_err(|e| RepoError::new(e.to_string()))?;
        sqlx::query(
            "INSERT OR REPLACE INTO remote_access \
             (database_id, enabled, allow_cidrs_json, wildcard_opt_in) \
             VALUES (?, ?, ?, ?)",
        )
        .bind(access.database_id.to_string())
        .bind(if access.enabled { 1 } else { 0 })
        .bind(cidrs)
        .bind(if access.wildcard_opt_in { 1 } else { 0 })
        .execute(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(())
    }

    async fn get_remote_access(
        &self,
        database_id: Uuid,
    ) -> Result<Option<RemoteAccess>, RepoError> {
        let row = sqlx::query(
            "SELECT database_id, enabled, allow_cidrs_json, wildcard_opt_in \
             FROM remote_access WHERE database_id = ?",
        )
        .bind(database_id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        row.map(decode_remote).transpose()
    }

    async fn list_remote_access(&self) -> Result<Vec<RemoteAccess>, RepoError> {
        let rows = sqlx::query(
            "SELECT database_id, enabled, allow_cidrs_json, wildcard_opt_in FROM remote_access",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        rows.into_iter().map(decode_remote).collect()
    }

    async fn save_sso_session(&self, session: &AdminToolSession) -> Result<(), RepoError> {
        let consumed_at = session.consumed_at.map(|t| t.to_rfc3339());
        sqlx::query(
            "INSERT OR REPLACE INTO admin_tool_sessions \
             (id, database_id, user_id, token, created_at, expires_at, consumed_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(session.id.to_string())
        .bind(session.database_id.to_string())
        .bind(session.user_id.to_string())
        .bind(&session.token)
        .bind(session.created_at.to_rfc3339())
        .bind(session.expires_at.to_rfc3339())
        .bind(consumed_at)
        .execute(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(())
    }

    async fn get_sso_session(&self, id: Uuid) -> Result<Option<AdminToolSession>, RepoError> {
        let row = sqlx::query(
            "SELECT id, database_id, user_id, token, created_at, expires_at, consumed_at \
             FROM admin_tool_sessions WHERE id = ?",
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        row.map(decode_sso).transpose()
    }

    async fn consume_sso_session(&self, id: Uuid) -> Result<(), RepoError> {
        sqlx::query("UPDATE admin_tool_sessions SET consumed_at = ? WHERE id = ?")
            .bind(Utc::now().to_rfc3339())
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(())
    }
}

fn decode_grant(row: sqlx::sqlite::SqliteRow) -> Result<DbGrant, RepoError> {
    let id: String = row.try_get("id").map_err(map_sqlx)?;
    let database_id: String = row.try_get("database_id").map_err(map_sqlx)?;
    let user_id: String = row.try_get("user_id").map_err(map_sqlx)?;
    let scope_kind: String = row.try_get("scope_kind").map_err(map_sqlx)?;
    let scope_name: Option<String> = row.try_get("scope_name").map_err(map_sqlx)?;
    let privilege: String = row.try_get("privilege").map_err(map_sqlx)?;
    let granted_by: String = row.try_get("granted_by").map_err(map_sqlx)?;
    let granted_at: String = row.try_get("granted_at").map_err(map_sqlx)?;
    let id = Uuid::parse_str(&id).map_err(|e| RepoError::new(e.to_string()))?;
    let database_id = Uuid::parse_str(&database_id).map_err(|e| RepoError::new(e.to_string()))?;
    let user_id = Uuid::parse_str(&user_id).map_err(|e| RepoError::new(e.to_string()))?;
    let scope = match scope_kind.as_str() {
        "database" => GrantScope::Database,
        "table" => GrantScope::Table {
            name: scope_name.ok_or_else(|| RepoError::new("missing table name".to_string()))?,
        },
        "routine" => GrantScope::Routine {
            name: scope_name.ok_or_else(|| RepoError::new("missing routine name".to_string()))?,
        },
        other => return Err(RepoError::new(format!("unknown scope kind: {other}"))),
    };
    let privilege = match privilege.as_str() {
        "read" => Privilege::Read,
        "write" => Privilege::Write,
        "ddl" => Privilege::Ddl,
        "grant" => Privilege::Grant,
        "all" => Privilege::All,
        other => return Err(RepoError::new(format!("unknown privilege: {other}"))),
    };
    let granted_by = Uuid::parse_str(&granted_by).map_err(|e| RepoError::new(e.to_string()))?;
    let granted_at = parse_ts(&granted_at)?;
    Ok(DbGrant {
        id,
        database_id,
        user_id,
        scope,
        privilege,
        granted_by,
        granted_at,
    })
}

fn decode_remote(row: sqlx::sqlite::SqliteRow) -> Result<RemoteAccess, RepoError> {
    let database_id: String = row.try_get("database_id").map_err(map_sqlx)?;
    let enabled: i64 = row.try_get("enabled").map_err(map_sqlx)?;
    let cidrs_json: String = row.try_get("allow_cidrs_json").map_err(map_sqlx)?;
    let wildcard_opt_in: i64 = row.try_get("wildcard_opt_in").map_err(map_sqlx)?;
    let database_id = Uuid::parse_str(&database_id).map_err(|e| RepoError::new(e.to_string()))?;
    let allow_cidrs: Vec<String> = serde_json::from_str(&cidrs_json)
        .map_err(|e| RepoError::new(format!("invalid cidrs: {e}")))?;
    Ok(RemoteAccess {
        database_id,
        enabled: enabled != 0,
        allow_cidrs,
        wildcard_opt_in: wildcard_opt_in != 0,
    })
}

fn decode_sso(row: sqlx::sqlite::SqliteRow) -> Result<AdminToolSession, RepoError> {
    let id: String = row.try_get("id").map_err(map_sqlx)?;
    let database_id: String = row.try_get("database_id").map_err(map_sqlx)?;
    let user_id: String = row.try_get("user_id").map_err(map_sqlx)?;
    let token: String = row.try_get("token").map_err(map_sqlx)?;
    let created_at: String = row.try_get("created_at").map_err(map_sqlx)?;
    let expires_at: String = row.try_get("expires_at").map_err(map_sqlx)?;
    let consumed_at: Option<String> = row.try_get("consumed_at").map_err(map_sqlx)?;
    let id = Uuid::parse_str(&id).map_err(|e| RepoError::new(e.to_string()))?;
    let database_id = Uuid::parse_str(&database_id).map_err(|e| RepoError::new(e.to_string()))?;
    let user_id = Uuid::parse_str(&user_id).map_err(|e| RepoError::new(e.to_string()))?;
    let created_at = parse_ts(&created_at)?;
    let expires_at = parse_ts(&expires_at)?;
    let consumed_at = consumed_at.as_deref().map(parse_ts).transpose()?;
    Ok(AdminToolSession {
        id,
        database_id,
        user_id,
        token,
        created_at,
        expires_at,
        consumed_at,
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
