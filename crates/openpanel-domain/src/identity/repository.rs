use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::{
    RepoError,
    identity::{
        factor::{Factor, TwoFactorChallenge, WebAuthnChallenge, WebAuthnCredential},
        role::Role,
        session::{Session, SessionToken},
        user::User,
    },
};

/// Persistence operations for users.
#[async_trait]
pub trait UserRepository: Send + Sync + 'static {
    /// Insert a new user.
    async fn insert(&self, user: &User) -> Result<(), RepoError>;
    /// Find a user by its identifier.
    async fn find_by_id(&self, id: Uuid) -> Result<Option<User>, RepoError>;
    /// Find a user by username.
    async fn find_by_username(&self, username: &str) -> Result<Option<User>, RepoError>;
    /// Find a user by email address.
    async fn find_by_email(&self, email: &str) -> Result<Option<User>, RepoError>;
    /// List all users.
    async fn list(&self) -> Result<Vec<User>, RepoError>;
    /// Update a user's role.
    async fn update_role(&self, id: Uuid, role: Role) -> Result<(), RepoError>;
    /// Disable a user account.
    async fn disable(&self, id: Uuid) -> Result<(), RepoError>;
    /// Re-enable a previously disabled user account (clears `disabled_at`).
    async fn enable(&self, id: Uuid) -> Result<(), RepoError>;
    /// Record the user's most recent login time.
    async fn update_last_login(&self, id: Uuid) -> Result<(), RepoError>;
    /// Update a user's password hash.
    async fn update_password(&self, id: Uuid, hash: &str) -> Result<(), RepoError>;
    /// Delete a user.
    async fn delete(&self, id: Uuid) -> Result<(), RepoError>;
    /// Count the number of users.
    async fn count(&self) -> Result<i64, RepoError>;
}

/// Persistence operations for sessions.
#[async_trait]
pub trait SessionRepository: Send + Sync + 'static {
    /// Insert a new session.
    async fn insert(&self, session: &Session) -> Result<(), RepoError>;
    /// Find a session by a matching token hash.
    async fn find_by_token_hash_match(
        &self,
        token: &SessionToken,
    ) -> Result<Option<Session>, RepoError>;
    /// Find a session by its identifier.
    async fn find_by_id(&self, id: Uuid) -> Result<Option<Session>, RepoError>;
    /// Refresh the session's activity timestamp.
    async fn touch(&self, id: Uuid) -> Result<(), RepoError>;
    /// Delete a session.
    async fn delete(&self, id: Uuid) -> Result<(), RepoError>;
    /// Delete all sessions belonging to a user.
    async fn delete_for_user(&self, user_id: Uuid) -> Result<(), RepoError>;
    /// Remove expired sessions and return the number removed.
    async fn purge_expired(&self) -> Result<u64, RepoError>;
}

/// Persistence operations for two-factor authentication: factors,
/// encrypted TOTP secrets, recovery codes, and pending-login
/// challenges. Phase A covers TOTP + recovery + challenges; WebAuthn
/// and remember-device are Phase B.
#[async_trait]
#[allow(clippy::too_many_arguments)]
pub trait FactorRepository: Send + Sync + 'static {
    // Factors
    /// Insert a new factor plus its encrypted TOTP secret (if TOTP).
    /// The `totp_secret_encrypted` argument is the empty string for
    /// non-TOTP factors.
    async fn insert_factor(
        &self,
        factor: &Factor,
        totp_secret_encrypted: &str,
    ) -> Result<(), RepoError>;
    /// Find a factor by id.
    async fn find_factor(&self, id: Uuid) -> Result<Option<Factor>, RepoError>;
    /// Update the factor (e.g. advance `last_used_step`, revoke).
    async fn update_factor(&self, factor: &Factor) -> Result<(), RepoError>;
    /// List a user's factors, including revoked ones. The caller
    /// filters by `revoked_at` if it only wants active factors.
    async fn list_factors_for_user(&self, user_id: Uuid) -> Result<Vec<Factor>, RepoError>;
    /// Return the first active TOTP factor for the user, if any.
    async fn find_active_totp_factor(&self, user_id: Uuid) -> Result<Option<Factor>, RepoError>;
    /// Return the encrypted TOTP secret for a factor, if any.
    async fn find_totp_secret_encrypted(
        &self,
        factor_id: Uuid,
    ) -> Result<Option<String>, RepoError>;

    // Recovery codes
    /// Replace the user's recovery-code set with a fresh batch.
    #[allow(clippy::type_complexity)]
    async fn replace_recovery_codes(
        &self,
        user_id: Uuid,
        entries: &[(String, DateTime<Utc>)],
    ) -> Result<(), RepoError>;
    /// Return the user's recovery codes as `(hash, consumed_at)` pairs.
    #[allow(clippy::type_complexity)]
    async fn list_recovery_codes(
        &self,
        user_id: Uuid,
    ) -> Result<Vec<(String, Option<DateTime<Utc>>)>, RepoError>;
    /// Mark a single recovery code as consumed (or no-op if already
    /// consumed). Identified by the plaintext (the repo hashes and
    /// compares).
    async fn consume_recovery_code(
        &self,
        user_id: Uuid,
        plaintext: &str,
        now: DateTime<Utc>,
    ) -> Result<bool, RepoError>;

    // Pending-login challenges
    /// Insert a pending-login challenge.
    async fn insert_challenge(&self, challenge: &TwoFactorChallenge) -> Result<(), RepoError>;
    /// Find a pending-login challenge by id.
    async fn find_challenge(&self, id: Uuid) -> Result<Option<TwoFactorChallenge>, RepoError>;
    /// Delete a pending-login challenge.
    async fn delete_challenge(&self, id: Uuid) -> Result<(), RepoError>;
    /// Verify the presented plaintext against the stored hash for the
    /// challenge. Returns `true` on match.
    async fn verify_challenge_token(
        &self,
        challenge_id: Uuid,
        plaintext: &str,
    ) -> Result<bool, RepoError>;
    /// Mark a challenge as consumed. Returns the user id on success.
    async fn consume_challenge(
        &self,
        challenge_id: Uuid,
        now: DateTime<Utc>,
    ) -> Result<Option<Uuid>, RepoError>;
    /// Remove expired challenges; returns the number removed.
    async fn purge_expired_challenges(&self, now: DateTime<Utc>) -> Result<u64, RepoError>;

    // WebAuthn credentials
    /// Insert a WebAuthn credential row.
    async fn insert_webauthn_credential(
        &self,
        credential_id: &str,
        user_id: Uuid,
        factor_id: Uuid,
        public_key_spki: &str,
        sign_count: i64,
        transports: Option<&'static str>,
        uv_policy: &str,
        created_at: DateTime<Utc>,
    ) -> Result<(), RepoError>;
    /// List the (non-revoked) WebAuthn credentials for a user.
    async fn list_webauthn_credentials(
        &self,
        user_id: Uuid,
    ) -> Result<Vec<WebAuthnCredential>, RepoError>;
    /// Look up a credential by its authenticator-issued id.
    async fn find_webauthn_credential_by_id(
        &self,
        credential_id: &str,
    ) -> Result<Option<WebAuthnCredential>, RepoError>;
    /// Increment `sign_count` and set `last_used_at` for a credential.
    async fn touch_webauthn_credential(
        &self,
        credential_id: &str,
        new_sign_count: i64,
        last_used_at: DateTime<Utc>,
    ) -> Result<(), RepoError>;
    /// Revoke a credential.
    async fn revoke_webauthn_credential(
        &self,
        credential_id: &str,
        now: DateTime<Utc>,
    ) -> Result<(), RepoError>;

    // WebAuthn ceremony challenge state (server-side state persisted
    // between the begin and finish halves of a registration or
    // assertion ceremony).
    /// Persist the opaque ceremony state for a pending challenge.
    async fn insert_webauthn_challenge(
        &self,
        id: Uuid,
        user_id: Uuid,
        kind: &str,
        state_json: &str,
        created_at: DateTime<Utc>,
        expires_at: DateTime<Utc>,
    ) -> Result<(), RepoError>;
    /// Load the ceremony state for a pending challenge.
    async fn find_webauthn_challenge(
        &self,
        id: Uuid,
    ) -> Result<Option<WebAuthnChallenge>, RepoError>;
    /// Mark a challenge as consumed.
    async fn consume_webauthn_challenge(
        &self,
        id: Uuid,
        now: DateTime<Utc>,
    ) -> Result<Option<String>, RepoError>;
}
