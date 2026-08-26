//! Database privilege management bounded context: per-user grant
//! scopes, remote access with an explicit `0.0.0.0/0` opt-in, and
//! short-lived single-use SSO tokens for the admin tool launcher.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::RepoError;

/// Errors raised by the DB privilege bounded context.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DbPrivilegeError {
    /// The caller is not authorised.
    #[error("forbidden")]
    Forbidden,
    /// The grant target is outside the database.
    #[error("grant outside database: {0}")]
    OutsideDatabase(String),
    /// The remote access ACL is empty and no opt-in was given.
    #[error("empty remote ACL requires explicit opt-in")]
    EmptyAcl,
    /// Wildcard opt-in (`0.0.0.0/0`) requires an explicit flag.
    #[error("wildcard remote ACL requires explicit opt-in")]
    WildcardAcl,
    /// The grant / remote / tool target does not exist.
    #[error("not found: {0}")]
    NotFound(String),
    /// The SSO token is invalid (expired or already consumed).
    #[error("invalid sso token")]
    InvalidSsoToken,
    /// A CIDR prefix is broader than the policy floor.
    #[error("cidr prefix too broad")]
    PrefixTooBroad,
    /// A remote-access input is malformed.
    #[error("invalid remote-access input: {0}")]
    InvalidPolicy(String),
    /// Grant application failed at the given 1-based step; earlier
    /// steps were reverted.
    #[error("grant failed at step {step}")]
    GrantFailed {
        /// The 1-based step that failed.
        step: usize,
    },
    /// Persistence failed.
    #[error("persistence failed: {0}")]
    Persistence(String),
}

pub mod remote_access;
pub use remote_access::{
    MysqlHostPattern, ReconcileStep, desired_hosts, mysql_host_pattern, reconcile_diff,
};

impl From<DbPrivilegeError> for RepoError {
    fn from(error: DbPrivilegeError) -> Self {
        RepoError::new(error.to_string())
    }
}

impl From<RepoError> for DbPrivilegeError {
    fn from(error: RepoError) -> Self {
        DbPrivilegeError::Persistence(error.0)
    }
}

/// Scope of a single grant row. `Table` and `Routine` are
/// sub-database scopes; the grant MUST lie inside the parent
/// database or `validate()` rejects it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GrantScope {
    /// Database-wide scope.
    Database,
    /// Per-table scope.
    Table {
        /// Table name (must be a safe identifier).
        name: String,
    },
    /// Per-routine scope.
    Routine {
        /// Routine name (must be a safe identifier).
        name: String,
    },
}

impl GrantScope {
    /// Stable lower-case label.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Database => "database",
            Self::Table { .. } => "table",
            Self::Routine { .. } => "routine",
        }
    }
}

/// Privilege verb. The `Read` / `Write` / `Ddl` / `Grant` verbs map
/// directly to SQL `GRANT` clauses; the audit log records the
/// verb but never the SQL text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Privilege {
    /// `SELECT`.
    Read,
    /// `INSERT` / `UPDATE` / `DELETE`.
    Write,
    /// `CREATE` / `DROP` / `ALTER`.
    Ddl,
    /// `GRANT OPTION`.
    Grant,
    /// All privileges (`ALL PRIVILEGES`).
    All,
}

impl Privilege {
    /// Stable lower-case label.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Write => "write",
            Self::Ddl => "ddl",
            Self::Grant => "grant",
            Self::All => "all",
        }
    }
}

/// A grant on a single (scope, privilege) pair.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DbGrant {
    /// Stable id.
    pub id: Uuid,
    /// Owning database.
    pub database_id: Uuid,
    /// Database user.
    pub user_id: Uuid,
    /// Scope (database, table, routine).
    pub scope: GrantScope,
    /// Privilege verb.
    pub privilege: Privilege,
    /// Principal that applied the grant.
    pub granted_by: Uuid,
    /// When the grant was applied.
    pub granted_at: DateTime<Utc>,
}

impl DbGrant {
    /// Validate the grant; `Table` and `Routine` scopes must
    /// name a safe identifier.
    pub fn validate(&self) -> Result<(), DbPrivilegeError> {
        match &self.scope {
            GrantScope::Database => Ok(()),
            GrantScope::Table { name } | GrantScope::Routine { name } => {
                if !is_safe_identifier(name) {
                    return Err(DbPrivilegeError::OutsideDatabase(name.clone()));
                }
                Ok(())
            }
        }
    }
}

/// Remote-access ACL.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteAccess {
    /// Owning database.
    pub database_id: Uuid,
    /// Whether remote access is enabled.
    pub enabled: bool,
    /// Allowed CIDR ranges. Empty list means "no remote access"
    /// and requires the `wildcard_opt_in` flag when toggling on
    /// for `0.0.0.0/0`.
    pub allow_cidrs: Vec<String>,
    /// Whether the operator has explicitly opted in to a
    /// `0.0.0.0/0` wildcard.
    pub wildcard_opt_in: bool,
}

impl RemoteAccess {
    /// Apply a request. Empty ACL + no opt-in is rejected.
    /// Wildcard ACL + no opt-in is rejected.
    pub fn from_request(
        database_id: Uuid,
        enabled: bool,
        allow_cidrs: Vec<String>,
        wildcard_opt_in: bool,
    ) -> Result<Self, DbPrivilegeError> {
        if enabled {
            if allow_cidrs.is_empty() {
                return Err(DbPrivilegeError::EmptyAcl);
            }
            if allow_cidrs.iter().any(|c| c.trim() == "0.0.0.0/0") && !wildcard_opt_in {
                return Err(DbPrivilegeError::WildcardAcl);
            }
        }
        Ok(Self {
            database_id,
            enabled,
            allow_cidrs,
            wildcard_opt_in,
        })
    }
}

/// A short-lived single-use SSO token for the admin tool launcher.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdminToolSession {
    /// Stable session id.
    pub id: Uuid,
    /// Owning database.
    pub database_id: Uuid,
    /// Database user the session authenticates as.
    pub user_id: Uuid,
    /// Random opaque token value.
    pub token: String,
    /// When the session was created.
    pub created_at: DateTime<Utc>,
    /// When the session expires (hard upper bound).
    pub expires_at: DateTime<Utc>,
    /// When the session was consumed (single-use).
    pub consumed_at: Option<DateTime<Utc>>,
}

impl AdminToolSession {
    /// Whether the session is still valid (not expired and not
    /// yet consumed).
    pub fn is_valid(&self, now: DateTime<Utc>) -> bool {
        self.consumed_at.is_none() && now <= self.expires_at
    }
}

/// Persistence port for the DB privilege bounded context.
#[async_trait]
pub trait DbPrivilegeRepository: Send + Sync + 'static {
    /// Persist a grant row.
    async fn save_grant(&self, grant: &DbGrant) -> Result<(), RepoError>;
    /// List grants for one database.
    async fn list_grants(&self, database_id: Uuid) -> Result<Vec<DbGrant>, RepoError>;
    /// List grants for one database user.
    async fn list_user_grants(
        &self,
        database_id: Uuid,
        user_id: Uuid,
    ) -> Result<Vec<DbGrant>, RepoError>;
    /// Delete a grant row.
    async fn delete_grant(&self, id: Uuid) -> Result<(), RepoError>;

    /// Persist the remote-access state for one database.
    async fn save_remote_access(&self, access: &RemoteAccess) -> Result<(), RepoError>;
    /// Load the remote-access state for one database.
    async fn get_remote_access(&self, database_id: Uuid)
    -> Result<Option<RemoteAccess>, RepoError>;
    /// List every stored remote-access state (boot reconcile).
    async fn list_remote_access(&self) -> Result<Vec<RemoteAccess>, RepoError>;

    /// Persist an admin-tool SSO session.
    async fn save_sso_session(&self, session: &AdminToolSession) -> Result<(), RepoError>;
    /// Load an SSO session by id.
    async fn get_sso_session(&self, id: Uuid) -> Result<Option<AdminToolSession>, RepoError>;
    /// Mark an SSO session as consumed.
    async fn consume_sso_session(&self, id: Uuid) -> Result<(), RepoError>;
}

fn is_safe_identifier(value: &str) -> bool {
    let bytes = value.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 64
        && bytes[0].is_ascii_alphabetic()
        && bytes
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || *b == b'_')
}
