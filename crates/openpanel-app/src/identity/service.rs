//! Identity application service: use cases for user/session management.

use std::sync::Arc;

use chrono::Utc;
use openpanel_core::{AuditAction, AuditEvent, AuditOutcome, AuditService};
use openpanel_domain::{
    Email, IdentityError, Password, Session, SessionRepository, SessionToken, User, UserRepository,
    Username, identity::role::Role,
};
use uuid::Uuid;

use crate::identity::{
    repo::{SqliteSessionRepository, SqliteUserRepository},
    two_factor::{ChallengeView, FactorResponse, TwoFactorError},
};

#[cfg(test)]
mod tests {
    //! Unit tests for `IdentityService` using `mockall` mocks at the
    //! repository / audit port boundaries. These tests run with no
    //! SQLite, no axum, no network — just the service logic over trait
    //! mocks.
    //!
    //! See `openspec/changes/add-tdd-infrastructure/tasks.md` § 2.4.

    use std::sync::Arc;

    use openpanel_core::{AuditAction, AuditOutcome};
    use openpanel_domain::{IdentityError, Password, Role, User};
    use openpanel_test_support::{MockAudit, MockFactorRepo, MockSessionRepo, MockUserRepo};
    use uuid::Uuid;

    use super::IdentityService;
    use crate::identity::two_factor::TwoFactorService;

    /// Build a disabled `User` row for the mock to return.
    fn disabled_user(username: &str) -> User {
        let email = openpanel_domain::Email::new(format!("{username}@example.com")).unwrap();
        let uname = openpanel_domain::Username::new(username.to_string()).unwrap();
        let pwd = Password::hash("correct horse battery staple").unwrap();
        let mut u = User::new(Uuid::new_v4(), uname, email, pwd, Role::Owner);
        u.disable();
        u
    }

    /// `login()` MUST return `AccountDisabled` (and audit the failure)
    /// when the user exists but is disabled. This is the unit-test
    /// equivalent of `tests/integration/identity::identity_login_disabled_user`,
    /// running without a database.
    #[tokio::test]
    async fn login_returns_account_disabled_when_user_is_disabled() {
        let user = disabled_user("alice");

        let mut users = MockUserRepo::new();
        users
            .expect_find_by_username()
            .times(1)
            .returning(move |_| Ok(Some(user.clone())));

        let sessions = MockSessionRepo::new();
        let factors = MockFactorRepo::new();

        let mut audit = MockAudit::new();
        audit.expect_record().times(1).returning(|event| {
            // The audit event MUST record the Login failure with the
            // `account_disabled` reason.
            assert_eq!(event.action, AuditAction::Login);
            assert_eq!(event.outcome, AuditOutcome::Failure);
            assert_eq!(
                event.metadata.get("reason").and_then(|v| v.as_str()),
                Some("account_disabled"),
            );
            Ok(())
        });
        // `recent` is never called during a disabled login; provide a
        // stub so the mock doesn't fail to satisfy an implicit
        // expectation.
        audit.expect_recent().returning(|_| Ok(vec![]));

        let two_factor = Arc::new(TwoFactorService::new(
            Arc::new(factors),
            [0u8; 32],
            Arc::new(MockAudit::stub()),
        ));
        let svc = IdentityService::new(
            Arc::new(users),
            Arc::new(sessions),
            Arc::new(audit),
            two_factor,
        );

        let err = svc
            .login("alice", "correct horse battery staple", None, None)
            .await
            .expect_err("login should fail");
        assert!(
            matches!(err, IdentityError::AccountDisabled),
            "expected AccountDisabled, got {err:?}"
        );
    }
}

/// Result of a password authentication. If the user has a second factor
/// enrolled, the outcome is `FactorRequired` and no session is created
/// until the factor challenge is satisfied.
#[derive(Debug, Clone)]
pub enum LoginOutcome {
    /// Password authenticated and no factor enrolled — a session has
    /// been issued.
    Authenticated {
        /// The authenticated user.
        user: User,
        /// Plaintext session token (the browser stores this).
        token: SessionToken,
    },
    /// Password authenticated but the user has a second factor enrolled;
    /// a challenge must be completed before a session is issued.
    FactorRequired {
        /// The authenticated user (the password step succeeded).
        user: User,
        /// The pending-login challenge for the second-factor step.
        challenge: ChallengeView,
    },
}

/// Application service orchestrating identity use cases (users, sessions).
#[derive(Clone)]
pub struct IdentityService {
    users: Arc<dyn UserRepository>,
    sessions: Arc<dyn SessionRepository>,
    audit: Arc<dyn AuditService>,
    two_factor: Arc<crate::identity::two_factor::TwoFactorService>,
}

impl IdentityService {
    /// Construct the service with the user + session repositories, audit
    /// sink, and the two-factor service used by the login flow.
    pub fn new(
        users: Arc<dyn UserRepository>,
        sessions: Arc<dyn SessionRepository>,
        audit: Arc<dyn AuditService>,
        two_factor: Arc<crate::identity::two_factor::TwoFactorService>,
    ) -> Self {
        Self {
            users,
            sessions,
            audit,
            two_factor,
        }
    }

    /// Return a clone of the underlying user repository handle.
    pub fn users(&self) -> Arc<dyn UserRepository> {
        self.users.clone()
    }

    /// Return a clone of the underlying session repository handle.
    pub fn sessions(&self) -> Arc<dyn SessionRepository> {
        self.sessions.clone()
    }

    /// Return a clone of the two-factor service handle.
    pub fn two_factor(&self) -> Arc<crate::identity::two_factor::TwoFactorService> {
        self.two_factor.clone()
    }

    /// Create a new user, hashing the plaintext password and auditing the action.
    pub async fn create_user(
        &self,
        username: &str,
        email: &str,
        plaintext_password: &str,
        role: Role,
        actor: &str,
    ) -> Result<User, IdentityError> {
        if Password::hash(plaintext_password).is_err() {
            self.audit
                .record(
                    AuditEvent::new(actor, AuditAction::UserCreated, AuditOutcome::Failure)
                        .metadata(serde_json::json!({"reason": "password_too_short"})),
                )
                .await
                .ok();
            return Err(IdentityError::PasswordTooShort);
        }

        let username = Username::new(username.to_string()).map_err(|_| IdentityError::Forbidden)?;
        let email = Email::new(email.to_string()).map_err(|_| IdentityError::Forbidden)?;
        let password =
            Password::hash(plaintext_password).map_err(|_| IdentityError::PasswordTooShort)?;

        if self
            .users
            .find_by_username(username.as_str())
            .await
            .ok()
            .flatten()
            .is_some()
        {
            return Err(IdentityError::UsernameTaken);
        }
        if self
            .users
            .find_by_email(email.as_str())
            .await
            .ok()
            .flatten()
            .is_some()
        {
            return Err(IdentityError::EmailTaken);
        }

        let user = User::new(Uuid::new_v4(), username, email, password, role);
        if let Err(e) = self.users.insert(&user).await {
            return Err(IdentityError::Persistence(e.0));
        }

        self.audit
            .record(
                AuditEvent::new(actor, AuditAction::UserCreated, AuditOutcome::Success)
                    .target(user.id().to_string()),
            )
            .await
            .ok();
        Ok(user)
    }

    /// Authenticate by username or email. The outcome is `Authenticated`
    /// when no second factor is enrolled (and a session is issued) or
    /// `FactorRequired` when the user has at least one enrolled factor
    /// (and a pending-login challenge is returned).
    pub async fn login(
        &self,
        username_or_email: &str,
        plaintext_password: &str,
        ip: Option<String>,
        user_agent: Option<String>,
    ) -> Result<LoginOutcome, IdentityError> {
        let user = self
            .users
            .find_by_username(username_or_email)
            .await
            .map_err(|e| IdentityError::Persistence(e.0))?;
        let user = match user {
            Some(u) => u,
            None => self
                .users
                .find_by_email(username_or_email)
                .await
                .map_err(|e| IdentityError::Persistence(e.0))?
                .ok_or(IdentityError::InvalidCredentials)?,
        };

        if user.is_disabled() {
            self.audit
                .record(
                    AuditEvent::new(
                        user.username().as_str(),
                        AuditAction::Login,
                        AuditOutcome::Failure,
                    )
                    .source_ip(ip.clone().unwrap_or_default())
                    .metadata(serde_json::json!({"reason": "account_disabled"})),
                )
                .await
                .ok();
            return Err(IdentityError::AccountDisabled);
        }

        let verified = user
            .password()
            .verify(plaintext_password)
            .map_err(|e| IdentityError::Persistence(e.to_string()))?;
        if !verified {
            self.audit
                .record(
                    AuditEvent::new(
                        user.username().as_str(),
                        AuditAction::Login,
                        AuditOutcome::Failure,
                    )
                    .source_ip(ip.clone().unwrap_or_default())
                    .metadata(serde_json::json!({"reason": "bad_credentials"})),
                )
                .await
                .ok();
            return Err(IdentityError::InvalidCredentials);
        }

        // Check whether the user has at least one enrolled second factor.
        let now = Utc::now();
        let has_factor = self
            .two_factor
            .has_active_factor(user.id())
            .await
            .map_err(|e| IdentityError::Persistence(e.to_string()))?;
        if has_factor {
            let challenge = self
                .two_factor
                .issue_login_challenge(user.id(), ip, user_agent.clone(), now)
                .await
                .map_err(|e| IdentityError::Persistence(e.to_string()))?;
            return Ok(LoginOutcome::FactorRequired { user, challenge });
        }

        // No factor enrolled — finish the session as before.
        let token = SessionToken::generate();
        let (mut session, token) = Session::new(user.id(), user.role(), &token, ip, user_agent);

        if let Err(e) = self.sessions.insert(&session).await {
            return Err(IdentityError::Persistence(e.0));
        }

        let _ = self.sessions.touch(session.id()).await;

        let mut user = user;
        user.record_login();
        let _ = self.users.update_last_login(user.id()).await;
        session.touch();

        self.audit
            .record(
                AuditEvent::new(
                    user.username().as_str(),
                    AuditAction::Login,
                    AuditOutcome::Success,
                )
                .source_ip(session.source_ip().unwrap_or("").to_string()),
            )
            .await
            .ok();

        Ok(LoginOutcome::Authenticated { user, token })
    }

    /// Complete a pending-login challenge by presenting a second-factor
    /// response. On success, a session is issued and returned.
    pub async fn verify_login_factor(
        &self,
        challenge_id: Uuid,
        challenge_token: &str,
        response: FactorResponse,
        ip: Option<String>,
        user_agent: Option<String>,
    ) -> Result<(User, SessionToken), IdentityError> {
        let now = Utc::now();
        let actor = "two_factor";
        let user_id = self
            .two_factor
            .verify_login_challenge(actor, challenge_id, challenge_token, response, now)
            .await
            .map_err(|e| match e {
                TwoFactorError::InvalidCode
                | TwoFactorError::CodeReplayed
                | TwoFactorError::NoFactor
                | TwoFactorError::Revoked => IdentityError::InvalidCredentials,
                other => IdentityError::Persistence(other.to_string()),
            })?;
        let Some(user) = self
            .users
            .find_by_id(user_id)
            .await
            .map_err(|e| IdentityError::Persistence(e.0))?
        else {
            return Err(IdentityError::UserNotFound);
        };
        if user.is_disabled() {
            return Err(IdentityError::AccountDisabled);
        }
        let token = SessionToken::generate();
        let (mut session, token) = Session::new(user.id(), user.role(), &token, ip, user_agent);
        if let Err(e) = self.sessions.insert(&session).await {
            return Err(IdentityError::Persistence(e.0));
        }
        let _ = self.sessions.touch(session.id()).await;
        let mut user = user;
        user.record_login();
        let _ = self.users.update_last_login(user.id()).await;
        session.touch();
        self.audit
            .record(
                AuditEvent::new(
                    user.username().as_str(),
                    AuditAction::Login,
                    AuditOutcome::Success,
                )
                .source_ip(session.source_ip().unwrap_or("").to_string()),
            )
            .await
            .ok();
        Ok((user, token))
    }

    /// Invalidate the session matching `token` and record a logout audit event.
    pub async fn logout(&self, token: &SessionToken, actor: &str) -> Result<(), IdentityError> {
        let session = self
            .sessions
            .find_by_token_hash_match(token)
            .await
            .map_err(|e| IdentityError::Persistence(e.0))?
            .ok_or(IdentityError::InvalidToken)?;
        self.sessions
            .delete(session.id())
            .await
            .map_err(|e| IdentityError::Persistence(e.0))?;
        self.audit
            .record(AuditEvent::new(
                actor,
                AuditAction::Logout,
                AuditOutcome::Success,
            ))
            .await
            .ok();
        Ok(())
    }

    /// Resolve a session token to its (user, session), expiring stale sessions.
    pub async fn resolve_session(
        &self,
        token: &SessionToken,
    ) -> Result<(User, Session), IdentityError> {
        let session = self
            .sessions
            .find_by_token_hash_match(token)
            .await
            .map_err(|e| IdentityError::Persistence(e.0))?
            .ok_or(IdentityError::InvalidToken)?;

        if session.is_expired(chrono::Utc::now()) {
            let _ = self.sessions.delete(session.id()).await;
            return Err(IdentityError::SessionExpired);
        }

        let user = self
            .users
            .find_by_id(session.user_id())
            .await
            .map_err(|e| IdentityError::Persistence(e.0))?
            .ok_or(IdentityError::UserNotFound)?;

        if user.is_disabled() {
            let _ = self.sessions.delete(session.id()).await;
            return Err(IdentityError::AccountDisabled);
        }

        Ok((user, session))
    }

    /// List every user in the repository.
    pub async fn list_users(&self) -> Result<Vec<User>, IdentityError> {
        self.users
            .list()
            .await
            .map_err(|e| IdentityError::Persistence(e.0))
    }

    /// Change a user's role, refusing to demote the last owner.
    pub async fn change_role(
        &self,
        target_id: Uuid,
        new_role: Role,
        actor: &str,
    ) -> Result<(), IdentityError> {
        let mut user = self
            .users
            .find_by_id(target_id)
            .await
            .map_err(|e| IdentityError::Persistence(e.0))?
            .ok_or(IdentityError::UserNotFound)?;
        user.change_role(new_role)
            .map_err(|_| IdentityError::LastOwner)?;
        self.users
            .update_role(user.id(), user.role())
            .await
            .map_err(|e| IdentityError::Persistence(e.0))?;
        self.audit
            .record(
                AuditEvent::new(actor, AuditAction::RoleChanged, AuditOutcome::Success)
                    .target(user.id().to_string())
                    .metadata(serde_json::json!({"new_role": new_role.as_str()})),
            )
            .await
            .ok();
        Ok(())
    }

    /// Disable a user account and purge all of their active sessions.
    pub async fn disable_user(&self, target_id: Uuid, actor: &str) -> Result<(), IdentityError> {
        let mut user = self
            .users
            .find_by_id(target_id)
            .await
            .map_err(|e| IdentityError::Persistence(e.0))?
            .ok_or(IdentityError::UserNotFound)?;
        user.disable();
        self.users
            .disable(user.id())
            .await
            .map_err(|e| IdentityError::Persistence(e.0))?;
        self.sessions
            .delete_for_user(user.id())
            .await
            .map_err(|e| IdentityError::Persistence(e.0))?;
        self.audit
            .record(
                AuditEvent::new(actor, AuditAction::UserDisabled, AuditOutcome::Success)
                    .target(user.id().to_string()),
            )
            .await
            .ok();
        Ok(())
    }

    /// Re-enable a previously disabled user account.
    pub async fn enable_user(&self, target_id: Uuid, actor: &str) -> Result<(), IdentityError> {
        let mut user = self
            .users
            .find_by_id(target_id)
            .await
            .map_err(|e| IdentityError::Persistence(e.0))?
            .ok_or(IdentityError::UserNotFound)?;
        user.enable();
        self.users
            .enable(user.id())
            .await
            .map_err(|e| IdentityError::Persistence(e.0))?;
        self.audit
            .record(
                AuditEvent::new(actor, AuditAction::UserEnabled, AuditOutcome::Success)
                    .target(user.id().to_string()),
            )
            .await
            .ok();
        Ok(())
    }

    /// Permanently delete a user (refuses owners and the last remaining user).
    pub async fn delete_user(&self, target_id: Uuid, actor: &str) -> Result<(), IdentityError> {
        let count = self
            .users
            .count()
            .await
            .map_err(|e| IdentityError::Persistence(e.0))?;
        if count <= 1 {
            return Err(IdentityError::LastOwner);
        }
        let user = self
            .users
            .find_by_id(target_id)
            .await
            .map_err(|e| IdentityError::Persistence(e.0))?
            .ok_or(IdentityError::UserNotFound)?;
        if matches!(user.role(), Role::Owner) {
            return Err(IdentityError::LastOwner);
        }
        self.sessions
            .delete_for_user(user.id())
            .await
            .map_err(|e| IdentityError::Persistence(e.0))?;
        self.users
            .delete(user.id())
            .await
            .map_err(|e| IdentityError::Persistence(e.0))?;
        self.audit
            .record(
                AuditEvent::new(actor, AuditAction::UserDeleted, AuditOutcome::Success)
                    .target(user.id().to_string()),
            )
            .await
            .ok();
        Ok(())
    }

    /// Change a user's password to a new plaintext value (hashed before persist).
    pub async fn change_password(
        &self,
        target_id: Uuid,
        new_plaintext: &str,
        actor: &str,
    ) -> Result<(), IdentityError> {
        let hash = Password::hash(new_plaintext).map_err(|_| IdentityError::PasswordTooShort)?;
        self.users
            .update_password(target_id, hash.hash_str())
            .await
            .map_err(|e| IdentityError::Persistence(e.0))?;
        self.audit
            .record(
                AuditEvent::new(actor, AuditAction::PasswordChanged, AuditOutcome::Success)
                    .target(target_id.to_string()),
            )
            .await
            .ok();
        Ok(())
    }
}

/// Convenience: build the SQLite-backed repositories from a pool.
pub fn build_repos(
    pool: sqlx::Pool<sqlx::Sqlite>,
) -> (Arc<SqliteUserRepository>, Arc<SqliteSessionRepository>) {
    (
        Arc::new(SqliteUserRepository::new(pool.clone())),
        Arc::new(SqliteSessionRepository::new(pool)),
    )
}
