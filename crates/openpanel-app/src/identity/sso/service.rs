//! SSO application service: connection management, login begin/
//! callback, and session inventory/revocation.

use std::sync::Arc;

use chrono::Utc;
use openpanel_core::{AuditAction, AuditEvent, AuditOutcome, AuditService};
use openpanel_domain::{
    Role, Session, SessionRepository, SessionToken, SsoConnection, SsoError, SsoLoginState, User,
    UserRepository,
};
use uuid::Uuid;

use super::OidcPort;
use crate::identity::two_factor::TwoFactorService;

/// Outcome of completing an SSO callback.
#[derive(Debug)]
pub enum CallbackOutcome {
    /// A session was minted. `mfa_satisfied` reports whether the IdP
    /// login is trusted to satisfy the panel's second factor.
    Session {
        /// The minted session token.
        token: SessionToken,
        /// Whether the second factor is satisfied by the IdP.
        mfa_satisfied: bool,
    },
    /// The user has a panel second factor enrolled and the connection
    /// does not trust the IdP's MFA: the challenge must be completed
    /// before a session is issued.
    FactorRequired {
        /// The pending-login challenge for the second-factor step.
        challenge: crate::identity::two_factor::ChallengeView,
    },
}

/// Owner-only SSO orchestration.
pub struct SsoService {
    repo: Arc<dyn openpanel_domain::SsoRepository>,
    users: Arc<dyn UserRepository>,
    sessions: Arc<dyn SessionRepository>,
    audit: Arc<dyn AuditService>,
    oidc: Arc<dyn OidcPort>,
    two_factor: Arc<TwoFactorService>,
    secret_key: [u8; 32],
}

impl SsoService {
    /// Construct with persistence, identity ports, the OIDC port, the
    /// second-factor service, and the master key used to encrypt the
    /// client secret at rest.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        repo: Arc<dyn openpanel_domain::SsoRepository>,
        users: Arc<dyn UserRepository>,
        sessions: Arc<dyn SessionRepository>,
        audit: Arc<dyn AuditService>,
        oidc: Arc<dyn OidcPort>,
        two_factor: Arc<TwoFactorService>,
        secret_key: [u8; 32],
    ) -> Self {
        Self {
            repo,
            users,
            sessions,
            audit,
            oidc,
            two_factor,
            secret_key,
        }
    }

    /// Load the configured connection.
    pub async fn connection(&self) -> Result<Option<SsoConnection>, SsoError> {
        self.repo.get_connection().await.map_err(persistence)
    }

    /// Configure (or replace) the singleton connection. Owner only.
    pub async fn configure(
        &self,
        caller: &User,
        mut connection: SsoConnection,
    ) -> Result<SsoConnection, SsoError> {
        if caller.role() != Role::Owner {
            return Err(SsoError::Forbidden);
        }
        // Encrypt a newly supplied plaintext secret; keep an existing
        // ciphertext untouched so re-saving other fields is possible.
        if !looks_like_cipher(&connection.client_secret_cipher) {
            connection.client_secret_cipher = crate::databases::crypto::encrypt_to_storage(
                &self.secret_key,
                &connection.client_secret_cipher,
            )
            .map_err(|error| SsoError::InvalidConfig(error.to_string()))?;
        }
        connection.validate()?;
        self.repo
            .save_connection(&connection)
            .await
            .map_err(persistence)?;
        Ok(connection)
    }

    /// Begin a login: mint state, persist it, return the provider
    /// authorization URL.
    pub async fn begin_login(&self) -> Result<String, SsoError> {
        let connection = self
            .repo
            .get_connection()
            .await
            .map_err(persistence)?
            .ok_or(SsoError::Discovery)?;
        let state = SsoLoginState::new(Utc::now());
        let url = self.oidc.authorize_url(&connection, &state).await?;
        self.repo.insert_state(&state).await.map_err(persistence)?;
        Ok(url)
    }

    /// Complete a login: validate state, exchange the code, link or
    /// provision the user, then either issue a session token or —
    /// when the connection does not trust the IdP's MFA and the user
    /// has a panel second factor enrolled — return the pending
    /// second-factor challenge.
    pub async fn callback(
        &self,
        code: &str,
        state_param: &str,
        ip: Option<String>,
        user_agent: Option<String>,
    ) -> Result<CallbackOutcome, SsoError> {
        let connection = self
            .repo
            .get_connection()
            .await
            .map_err(persistence)?
            .ok_or(SsoError::Discovery)?;
        let outstanding = self
            .repo
            .take_state(state_param)
            .await
            .map_err(persistence)?
            .ok_or(SsoError::StateMismatch)?;
        outstanding.validate_state(state_param, Utc::now())?;

        let claims = self
            .oidc
            .exchange(
                &connection,
                code,
                &outstanding.pkce_verifier,
                &outstanding.nonce,
            )
            .await?;

        let issuer = claims.issuer.to_lowercase();
        let existing = self
            .repo
            .find_identity(&issuer, &claims.subject)
            .await
            .map_err(persistence)?;
        let user_id = match existing {
            Some(identity) => identity.user_id,
            None => {
                if !connection.auto_provision {
                    return Err(SsoError::ProvisionDisabled);
                }
                let stem: String = claims
                    .subject
                    .chars()
                    .filter(|character| character.is_ascii_alphanumeric())
                    .take(12)
                    .collect();
                let username = format!("sso-{stem}");
                let email = openpanel_domain::Email::new(
                    claims
                        .email
                        .clone()
                        .unwrap_or_else(|| format!("{username}@sso.local")),
                )
                .map_err(|_| SsoError::InvalidConfig("provider email invalid".into()))?;
                let uname = openpanel_domain::Username::new(username.clone())
                    .map_err(|_| SsoError::InvalidConfig("derived username invalid".into()))?;
                // Provisioned accounts get an unusable random password;
                // login happens exclusively through the IdP.
                let password = openpanel_domain::Password::hash(&crate::random_token())
                    .map_err(|error| SsoError::InvalidConfig(error.to_string()))?;
                let user = User::new(
                    Uuid::new_v4(),
                    uname,
                    email,
                    password,
                    connection.default_role,
                );
                self.users.insert(&user).await.map_err(persistence)?;
                self.repo
                    .save_identity(&openpanel_domain::ExternalIdentity {
                        issuer,
                        subject: claims.subject.clone(),
                        user_id: user.id(),
                    })
                    .await
                    .map_err(persistence)?;
                user.id()
            }
        };

        let user = self
            .users
            .find_by_id(user_id)
            .await
            .map_err(persistence)?
            .ok_or(SsoError::LinkConflict)?;

        // When the connection does not trust the IdP's MFA, a user
        // with an enrolled panel second factor must complete the
        // existing challenge before any session is minted.
        if !connection.mfa_satisfied() {
            let has_factor = self
                .two_factor
                .has_active_factor(user.id())
                .await
                .map_err(|error| SsoError::InvalidConfig(error.to_string()))?;
            if has_factor {
                let challenge = self
                    .two_factor
                    .issue_login_challenge(user.id(), ip, user_agent, Utc::now())
                    .await
                    .map_err(|error| SsoError::InvalidConfig(error.to_string()))?;
                return Ok(CallbackOutcome::FactorRequired { challenge });
            }
        }

        let token = SessionToken::generate();
        let (mut session, session_token) = Session::new(
            user.id(),
            user.role(),
            &token,
            ip.clone(),
            user_agent.clone(),
        );
        self.sessions.insert(&session).await.map_err(persistence)?;
        session.touch();
        let _ = self
            .audit
            .record(
                AuditEvent::new(
                    user.username().as_str(),
                    AuditAction::SsoLogin,
                    AuditOutcome::Success,
                )
                .metadata(serde_json::json!({ "issuer": claims.issuer })),
            )
            .await;
        Ok(CallbackOutcome::Session {
            token: session_token,
            mfa_satisfied: connection.mfa_satisfied(),
        })
    }

    /// List a caller's own active sessions.
    pub async fn list_sessions(&self, caller: &User) -> Result<Vec<Session>, SsoError> {
        self.sessions
            .list_for_user(caller.id())
            .await
            .map_err(persistence)
    }

    /// Revoke one of the caller's own sessions.
    pub async fn revoke_own_session(
        &self,
        caller: &User,
        session_id: Uuid,
    ) -> Result<(), SsoError> {
        let owned = self
            .sessions
            .list_for_user(caller.id())
            .await
            .map_err(persistence)?
            .iter()
            .any(|session| session.id == session_id);
        if !owned {
            return Err(SsoError::StateMismatch);
        }
        self.sessions
            .delete(session_id)
            .await
            .map_err(persistence)?;
        self.audit_session_revoked(caller.username().as_str(), session_id)
            .await;
        Ok(())
    }

    /// Admin override: revoke any user's session.
    pub async fn revoke_user_session(
        &self,
        caller: &User,
        target_user: Uuid,
        session_id: Uuid,
    ) -> Result<(), SsoError> {
        if caller.role() != Role::Owner && caller.role() != Role::Admin {
            return Err(SsoError::Forbidden);
        }
        let owned = self
            .sessions
            .list_for_user(target_user)
            .await
            .map_err(persistence)?
            .iter()
            .any(|session| session.id == session_id);
        if !owned {
            return Err(SsoError::StateMismatch);
        }
        self.sessions
            .delete(session_id)
            .await
            .map_err(persistence)?;
        self.audit_session_revoked(caller.username().as_str(), session_id)
            .await;
        Ok(())
    }

    async fn audit_session_revoked(&self, actor: &str, session_id: Uuid) {
        let _ = self
            .audit
            .record(
                AuditEvent::new(actor, AuditAction::SessionRevoked, AuditOutcome::Success)
                    .target(session_id.to_string()),
            )
            .await;
    }
}

fn looks_like_cipher(value: &str) -> bool {
    let parts: Vec<&str> = value.split(':').collect();
    parts.len() == 2
        && parts
            .iter()
            .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_hexdigit()))
}

fn persistence(error: impl std::fmt::Display) -> SsoError {
    SsoError::Persistence(error.to_string())
}
