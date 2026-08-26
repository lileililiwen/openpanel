//! Database privilege services: grant application, remote-access
//! controller, and the admin-tool SSO launcher.

use std::sync::Arc;

use chrono::{Duration, Utc};
use openpanel_core::{AuditAction, AuditEvent, AuditOutcome, AuditService};
use openpanel_domain::{
    AdminToolSession, DbGrant, DbPrivilegeError, DbPrivilegeRepository, RemoteAccess, Role, User,
    db_privileges::ReconcileStep,
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
    pub async fn apply(&self, caller: &User, grant: DbGrant) -> Result<DbGrant, DbPrivilegeError> {
        require_admin(caller)?;
        grant.validate()?;
        self.repo.save_grant(&grant).await?;
        let _ = self
            .audit
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
        let _ = self
            .audit
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
        let access =
            RemoteAccess::from_request(database_id, enabled, allow_cidrs, wildcard_opt_in)?;
        self.repo.save_remote_access(&access).await?;
        let _ = self
            .audit
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
        let _ = self
            .audit
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

impl RemoteAccessController {
    /// Boot reconcile: re-apply every stored ACL through the port so
    /// manually emptied grant tables self-heal. Returns the number of
    /// databases reconciled.
    pub async fn reconcile_all(
        &self,
        accounts: &[(Uuid, String, String)],
        port: &dyn MySqlGrantPort,
    ) -> Result<usize, DbPrivilegeError>
    where
        Self: Sized,
    {
        let stored = self.repo.list_remote_access().await?;
        let mut count = 0;
        for access in &stored {
            let Some((user, database)) = accounts
                .iter()
                .find(|(id, _, _)| *id == access.database_id)
                .map(|(_, u, d)| (u.clone(), d.clone()))
            else {
                continue;
            };
            let desired = if access.enabled {
                openpanel_domain::db_privileges::desired_hosts(
                    &access.allow_cidrs,
                    access.wildcard_opt_in,
                )?
            } else {
                continue;
            };
            let current = port.current_hosts(&user, &database).await?;
            for step in openpanel_domain::db_privileges::reconcile_diff(&desired, &current) {
                match step {
                    ReconcileStep::Create { host } => {
                        port.create_user_host(&user, &host, &database).await?;
                    }
                    ReconcileStep::Drop { host } => {
                        port.drop_user_host(&user, &host).await?;
                    }
                }
            }
            count += 1;
        }
        Ok(count)
    }
}

/// Shared state for the remote-access REST surface: the controller
/// plus whichever grant port the composition root chose.
#[derive(Clone)]
pub struct DbRemoteAccessContext {
    /// The controller executing all-or-nothing applies.
    pub controller: Arc<RemoteAccessController>,
    /// The configured grant port (shell-out in production, recorder
    /// in tests).
    pub port: Arc<dyn MySqlGrantPort>,
}

/// Port driving the live MySQL grant table. Implemented by the
/// mysql shell-out adapter; tests inject recorders.
#[async_trait::async_trait]
pub trait MySqlGrantPort: Send + Sync + 'static {
    /// Hosts currently holding grants for `user` on `database`.
    async fn current_hosts(
        &self,
        user: &str,
        database: &str,
    ) -> Result<Vec<String>, DbPrivilegeError>;
    /// Create `user@host` with ALL on `database`.*.
    async fn create_user_host(
        &self,
        user: &str,
        host: &str,
        database: &str,
    ) -> Result<(), DbPrivilegeError>;
    /// Drop `user@host`.
    async fn drop_user_host(&self, user: &str, host: &str) -> Result<(), DbPrivilegeError>;
}

impl RemoteAccessController {
    /// Apply an ACL through the grant port: derive desired hosts,
    /// diff against live state, execute all-or-nothing (a failing
    /// step reverts its predecessors in reverse order), persist the
    /// applied patterns, and audit.
    #[allow(clippy::too_many_arguments)]
    pub async fn apply(
        &self,
        caller: &User,
        database_id: Uuid,
        user: &str,
        database: &str,
        enabled: bool,
        allow_cidrs: Vec<String>,
        wildcard_opt_in: bool,
        port: &dyn MySqlGrantPort,
    ) -> Result<RemoteAccess, DbPrivilegeError> {
        let access =
            RemoteAccess::from_request(database_id, enabled, allow_cidrs, wildcard_opt_in)?;
        let desired = if access.enabled {
            openpanel_domain::db_privileges::desired_hosts(
                &access.allow_cidrs,
                access.wildcard_opt_in,
            )?
        } else {
            vec!["localhost".to_string()]
        };
        let current = port.current_hosts(user, database).await?;
        let steps = openpanel_domain::db_privileges::reconcile_diff(&desired, &current);

        let mut applied: Vec<ReconcileStep> = Vec::new();
        for (index, step) in steps.iter().enumerate() {
            let result = match step {
                ReconcileStep::Create { host } => port.create_user_host(user, host, database).await,
                ReconcileStep::Drop { host } => port.drop_user_host(user, host).await,
            };
            if let Err(error) = result {
                // All-or-nothing: undo predecessors in reverse.
                for done in applied.drain(..).rev() {
                    match done {
                        ReconcileStep::Create { host } => {
                            let _ = port.drop_user_host(user, &host).await;
                        }
                        ReconcileStep::Drop { host } => {
                            let _ = port.create_user_host(user, &host, database).await;
                        }
                    }
                }
                let _ = error;
                return Err(DbPrivilegeError::GrantFailed { step: index + 1 });
            }
            applied.push(step.clone());
        }

        self.repo.save_remote_access(&access).await?;
        let added: Vec<String> = steps
            .iter()
            .filter_map(|s| match s {
                ReconcileStep::Create { host } => Some(host.clone()),
                _ => None,
            })
            .collect();
        let removed: Vec<String> = steps
            .iter()
            .filter_map(|s| match s {
                ReconcileStep::Drop { host } => Some(host.clone()),
                _ => None,
            })
            .collect();
        let _ = self
            .audit
            .record(
                AuditEvent::new(
                    caller.username().as_str(),
                    AuditAction::DbRemoteAccessChanged,
                    AuditOutcome::Success,
                )
                .target(database_id.to_string())
                .metadata(serde_json::json!({
                    "enabled": access.enabled,
                    "added": added,
                    "removed": removed,
                })),
            )
            .await;
        Ok(access)
    }
}

#[cfg(test)]
mod remote_access_apply_tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use std::sync::Mutex;

    use openpanel_test_support::MockAudit;

    use super::*;

    struct RecordingPort {
        /// Every executed call in order ("create:host" / "drop:host").
        calls: Mutex<Vec<String>>,
        /// Number of attempted statements.
        attempts: Mutex<usize>,
        /// 1-based attempt index that fails (0 = none).
        fail_at: usize,
    }

    impl RecordingPort {
        fn new(fail_at: usize) -> Self {
            Self {
                calls: Mutex::new(Vec::new()),
                attempts: Mutex::new(0),
                fail_at,
            }
        }

        fn order(&self) -> Vec<String> {
            self.calls.lock().unwrap().clone()
        }

        fn attempt(&self) -> Result<(), DbPrivilegeError> {
            let mut attempts = self.attempts.lock().unwrap();
            *attempts += 1;
            if *attempts == self.fail_at {
                Err(DbPrivilegeError::GrantFailed { step: 0 })
            } else {
                Ok(())
            }
        }
    }

    #[async_trait::async_trait]
    impl MySqlGrantPort for RecordingPort {
        async fn current_hosts(
            &self,
            _user: &str,
            _database: &str,
        ) -> Result<Vec<String>, DbPrivilegeError> {
            Ok(vec!["localhost".to_string()])
        }

        async fn create_user_host(
            &self,
            _user: &str,
            host: &str,
            _database: &str,
        ) -> Result<(), DbPrivilegeError> {
            self.attempt()?;
            self.calls.lock().unwrap().push(format!("create:{host}"));
            Ok(())
        }

        async fn drop_user_host(&self, _user: &str, host: &str) -> Result<(), DbPrivilegeError> {
            self.attempt()?;
            self.calls.lock().unwrap().push(format!("drop:{host}"));
            Ok(())
        }
    }

    fn controller(
        runtime: &tokio::runtime::Runtime,
    ) -> (RemoteAccessController, openpanel_test_support::TestDb) {
        let db = runtime.block_on(openpanel_test_support::TestDb::new());
        // The repo needs the privileges schema.
        runtime.block_on(async {
            sqlx::raw_sql(crate::migrations::DB_PRIVILEGES_V001)
                .execute(&db.pool())
                .await
                .unwrap();
        });
        (
            RemoteAccessController::new(
                Arc::new(crate::db_privileges::SqliteDbPrivilegeRepository::new(
                    db.pool(),
                )),
                Arc::new(MockAudit::stub()),
            ),
            db,
        )
    }

    fn owner() -> User {
        use openpanel_domain::{Email, Password, Username};
        User::new(
            Uuid::new_v4(),
            Username::new("owner").unwrap(),
            Email::new("owner@example.test").unwrap(),
            Password::hash("correct horse battery staple").unwrap(),
            Role::Owner,
        )
    }

    #[test]
    fn apply_is_all_or_nothing_with_revert_on_failure() {
        openpanel_domain::Password::set_test_costs(8, 1);
        let caller = owner();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let (controller, _db) = controller(&runtime);
        runtime.block_on(async {
            let port = RecordingPort::new(2); // second statement fails
            let error = match controller
                .apply(
                    &caller,
                    Uuid::new_v4(),
                    "wp1",
                    "wp1db",
                    true,
                    vec!["203.0.113.0/24".into(), "198.51.100.7/32".into()],
                    false,
                    &port,
                )
                .await
            {
                Err(error) => error,
                Ok(_) => panic!("apply must fail"),
            };
            assert!(
                matches!(error, DbPrivilegeError::GrantFailed { step: 2 }),
                "expected GrantFailed{{step:2}}, got {error}"
            );
            // Call order: create p1, fail at create p2, revert drop p1.
            assert_eq!(
                port.order(),
                vec![
                    "create:203.0.113.%".to_string(),
                    "drop:203.0.113.%".to_string(),
                ]
            );
        });
    }

    #[test]
    fn apply_creates_and_drops_in_order_on_success() {
        openpanel_domain::Password::set_test_costs(8, 1);
        let caller = owner();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let (controller, _db) = controller(&runtime);
        runtime.block_on(async {
            let port = RecordingPort::new(0);
            // Current has an extra stale host → one drop; ACL adds one
            // pattern → one create.
            let access = controller
                .apply(
                    &caller,
                    Uuid::new_v4(),
                    "wp1",
                    "wp1db",
                    true,
                    vec!["203.0.113.0/24".into()],
                    false,
                    &port,
                )
                .await
                .unwrap();
            assert!(access.enabled);
            assert_eq!(port.order(), vec!["create:203.0.113.%".to_string()]);
        });
    }
}
