//! Database privilege services: grant application, remote-access
//! controller, and the admin-tool SSO launcher.

use std::sync::Arc;

use chrono::{Duration, Utc};
use openpanel_core::{AuditAction, AuditEvent, AuditOutcome, AuditService};
use openpanel_domain::{
    AdminToolSession, DbGrant, DbPrivilegeError, DbPrivilegeRepository, GrantScope, Privilege,
    RemoteAccess, Role, User,
};
use uuid::Uuid;

use crate::db_privileges::SqliteDbPrivilegeRepository;

/// Default SSO session lifetime (5 minutes).
pub const DEFAULT_SSO_TTL_SECS: i64 = 300;

/// Apply, revoke, and list grants for a database user.
pub struct PrivilegeService {
    repo: Arc<SqliteDbPrivilegeRepository>,
    audit: Arc<dyn AuditService>,
}

impl PrivilegeService {
    /// Construct a privilege service.
    pub fn new(repo: Arc<SqliteDbPrivilegeRepository>, audit: Arc<dyn AuditService>) -> Self {
        Self { repo, audit }
    }

    /// Apply a new grant. Caller MUST be Admin/Owner.
    pub async fn apply(
        &self,
        caller: &User,
        mut grant: DbGrant,
    ) -> Result<DbGrant, DbPrivilegeError> {
        require_admin(caller)?;
        grant.validate()?;
        self.repo.save_grant(&grant).await?;
        self.audit
            .record(
                AuditEvent::new(
                    caller.username().as_str(),
                    AuditAction::DbPrivilegeGranted,
                    AuditOutcome::Success,
                )
                .target(grant.database_id.to_string())
                .metadata(serde_json::json!({
                    "privilege": grant.privilege.as_str(),
                    "scope_kind": grant.scope.kind(),
                    "user_id": grant.user_id.to_string(),
                })),
            )
            .await;
        Ok(grant)
    }

    /// Revoke a grant by id.
    pub async fn revoke(&self, caller: &User, id: Uuid) -> Result<(), DbPrivilegeError> {
        require_admin(caller)?;
        self.repo.delete_grant(id).await?;
        self.audit
            .record(
                AuditEvent::new(
                    caller.username().as_str(),
                    AuditAction::DbPrivilegeRevoked,
                    AuditOutcome::Success,
                )
                .target(id.to_string()),
            )
            .await;
        Ok(())
    }

    /// List all grants for one database.
    pub async fn list(
        &self,
        caller: &User,
        database_id: Uuid,
    ) -> Result<Vec<DbGrant>, DbPrivilegeError> {
        require_admin(caller)?;
        Ok(self.repo.list_grants(database_id).await?)
    }

    /// List grants for one database user.
    pub async fn list_user(
        &self,
        caller: &User,
        database_id: Uuid,
        user_id: Uuid,
    ) -> Result<Vec<DbGrant>, DbPrivilegeError> {
        require_admin(caller)?;
        Ok(self.repo.list_user_grants(database_id, user_id).await?)
    }
}

/// Remote access controller: enable/disable + ACL enforcement.
pub struct RemoteAccessController {
    repo: Arc<SqliteDbPrivilegeRepository>,
    audit: Arc<dyn AuditService>,
}

impl RemoteAccessController {
    /// Construct a controller.
    pub fn new(repo: Arc<SqliteDbPrivilegeRepository>, audit: Arc<dyn AuditService>) -> Self {
        Self { repo, audit }
    }

    /// Load the current state, or return a default (disabled).
    pub async fn get(
        &self,
        caller: &User,
        database_id: Uuid,
    ) -> Result<RemoteAccess, DbPrivilegeError> {
        require_admin(caller)?;
        Ok(self
            .repo
            .get_remote_access(database_id)
            .await?
            .unwrap_or(RemoteAccess {
                database_id,
                enabled: false,
                allow_cidrs: Vec::new(),
                wildcard_opt_in: false,
            }))
    }

    /// Set the remote-access state. Empty ACLs and `0.0.0.0/0`
    /// without the opt-in flag are rejected.
    pub async fn set(
        &self,
        caller: &User,
        database_id: Uuid,
        enabled: bool,
        allow_cidrs: Vec<String>,
        wildcard_opt_in: bool,
    ) -> Result<RemoteAccess, DbPrivilegeError> {
        require_admin(caller)?;
        let access = RemoteAccess::from_request(
            database_id,
            enabled,
            allow_cidrs,
            wildcard_opt_in,
        )?;
        self.repo.save_remote_access(&access).await?;
        self.audit
            .record(
                AuditEvent::new(
                    caller.username().as_str(),
                    AuditAction::DbRemoteAccessChanged,
                    AuditOutcome::Success,
                )
                .target(database_id.to_string())
                .metadata(serde_json::json!({
                    "enabled": access.enabled,
                    "cidrs": access.allow_cidrs,
                    "wildcard_opt_in": access.wildcard_opt_in,
                })),
            )
            .await;
        Ok(access)
    }
}

/// SSO token issuer for the admin tool launcher. Tokens are
/// single-use and bounded by a TTL.
pub struct AdminToolSso {
    repo: Arc<SqliteDbPrivilegeRepository>,
    audit: Arc<dyn AuditService>,
    ttl: Duration,
}

impl AdminToolSso {
    /// Construct an SSO issuer with the default TTL.
    pub fn new(repo: Arc<SqliteDbPrivilegeRepository>, audit: Arc<dyn AuditService>) -> Self {
        Self {
            repo,
            audit,
            ttl: Duration::seconds(DEFAULT_SSO_TTL_SECS),
        }
    }

    /// Override the TTL (test wiring).
    pub fn with_ttl(mut self, ttl: Duration) -> Self {
        self.ttl = ttl;
        self
    }

    /// Issue a new session for `caller` on `database_id` for
    /// `user_id`. Returns the freshly-minted session.
    pub async fn issue(
        &self,
        caller: &User,
        database_id: Uuid,
        user_id: Uuid,
    ) -> Result<AdminToolSession, DbPrivilegeError> {
        require_admin(caller)?;
        let now = Utc::now();
        let session = AdminToolSession {
            id: Uuid::new_v4(),
            database_id,
            user_id,
            token: format!("sso-{}", Uuid::new_v4().simple()),
            created_at: now,
            expires_at: now + self.ttl,
            consumed_at: None,
        };
        self.repo.save_sso_session(&session).await?;
        self.audit
            .record(
                AuditEvent::new(
                    caller.username().as_str(),
                    AuditAction::DbAdminToolLaunched,
                    AuditOutcome::Success,
                )
                .target(database_id.to_string())
                .metadata(serde_json::json!({
                    "session_id": session.id.to_string(),
                    "expires_at": session.expires_at.to_rfc3339(),
                })),
            )
            .await;
        Ok(session)
    }

    /// Consume a session. Returns `DbPrivilegeError::InvalidSsoToken`
    /// when the session is expired or already consumed.
    pub async fn consume(
        &self,
        caller: &User,
        session_id: Uuid,
        presented_token: &str,
    ) -> Result<AdminToolSession, DbPrivilegeError> {
        require_admin(caller)?;
        let session = self
            .repo
            .get_sso_session(session_id)
            .await?
            .ok_or(DbPrivilegeError::InvalidSsoToken)?;
        if !session.is_valid(Utc::now()) {
            return Err(DbPrivilegeError::InvalidSsoToken);
        }
        if session.token != presented_token {
            return Err(DbPrivilegeError::InvalidSsoToken);
        }
        self.repo.consume_sso_session(session.id).await?;
        Ok(AdminToolSession {
            consumed_at: Some(Utc::now()),
            ..session
        })
    }
}

fn require_admin(caller: &User) -> Result<(), DbPrivilegeError> {
    match caller.role() {
        Role::Owner | Role::Admin => Ok(()),
        _ => Err(DbPrivilegeError::Forbidden),
    }
}
