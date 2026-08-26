//! OpenID Connect login for the panel plus cross-session control.
//! Pure domain — no HTTP, no network.

use chrono::{DateTime, Duration, Utc};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{RepoError, Role};

/// State rows live for ten minutes.
pub const STATE_TTL_SECS: i64 = 600;

/// Errors raised by the SSO bounded context.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SsoError {
    /// Only Owners may configure the connection.
    #[error("forbidden")]
    Forbidden,
    /// The connection configuration violates an invariant.
    #[error("invalid SSO configuration: {0}")]
    InvalidConfig(String),
    /// The provider's discovery document could not be loaded.
    #[error("provider discovery failed")]
    Discovery,
    /// The callback state does not match an outstanding login.
    #[error("state mismatch")]
    StateMismatch,
    /// The outstanding state expired before the callback arrived.
    #[error("state expired")]
    StateExpired,
    /// The ID-token nonce did not match the issued nonce.
    #[error("nonce mismatch")]
    NonceMismatch,
    /// The identity is unlinked and auto-provisioning is disabled.
    #[error("auto-provision disabled")]
    ProvisionDisabled,
    /// The external identity is already linked to another user.
    #[error("identity already linked")]
    LinkConflict,
    /// Persistence failed.
    #[error("persistence failed: {0}")]
    Persistence(String),
}

/// Panel-side OIDC connection configuration. The client secret is
/// stored only as AES-256-GCM ciphertext in the documented
/// `hex(nonce):hex(ciphertext)` layout.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SsoConnection {
    /// Issuer base URL, e.g. `https://idp.example.com`.
    pub issuer_url: String,
    /// OAuth client id.
    pub client_id: String,
    /// Encrypted client secret (never plaintext at rest).
    pub client_secret_cipher: String,
    /// Role assigned to auto-provisioned users.
    pub default_role: Role,
    /// Create a local user on first login of an unknown identity.
    pub auto_provision: bool,
    /// Whether an MFA assertion at the IdP satisfies the panel's
    /// second-factor challenge.
    pub trust_idp_mfa: bool,
}

impl Default for SsoConnection {
    fn default() -> Self {
        Self {
            issuer_url: String::new(),
            client_id: String::new(),
            client_secret_cipher: String::new(),
            default_role: Role::User,
            auto_provision: false,
            trust_idp_mfa: false,
        }
    }
}

impl SsoConnection {
    /// Validate issuer/client fields and the ciphertext layout.
    pub fn validate(&self) -> Result<(), SsoError> {
        if self.issuer_url.is_empty()
            || self.issuer_url.len() > 512
            || !(self.issuer_url.starts_with("https://") || self.issuer_url.starts_with("http://"))
        {
            return Err(SsoError::InvalidConfig(
                "issuer_url must be an http(s) URL".into(),
            ));
        }
        if self.client_id.is_empty() || self.client_id.len() > 256 {
            return Err(SsoError::InvalidConfig("client_id is required".into()));
        }
        let parts: Vec<&str> = self.client_secret_cipher.split(':').collect();
        let shaped = parts.len() == 2
            && !parts[0].is_empty()
            && !parts[1].is_empty()
            && parts[0].bytes().all(|byte| byte.is_ascii_hexdigit())
            && parts[1].bytes().all(|byte| byte.is_ascii_hexdigit());
        if !shaped {
            return Err(SsoError::InvalidConfig(
                "client secret must be stored as hex(nonce):hex(ciphertext)".into(),
            ));
        }
        Ok(())
    }

    /// Whether a session minted through this connection may skip the
    /// panel's second-factor challenge.
    pub fn mfa_satisfied(&self) -> bool {
        self.trust_idp_mfa
    }
}

/// A link between an IdP identity and a local user.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalIdentity {
    /// Issuer URL (lowercased for comparisons).
    pub issuer: String,
    /// The IdP's stable subject claim.
    pub subject: String,
    /// Linked local user.
    pub user_id: Uuid,
}

/// One outstanding login attempt. Consumed on callback.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SsoLoginState {
    /// Random state parameter (≥32 hex chars).
    pub state: String,
    /// Random nonce bound into the ID token.
    pub nonce: String,
    /// PKCE verifier kept server-side.
    pub pkce_verifier: String,
    /// When the row expires.
    pub expires_at: DateTime<Utc>,
}

fn random_hex(bytes: usize) -> String {
    let mut buf = vec![0u8; bytes];
    rand::rngs::OsRng.fill_bytes(&mut buf);
    buf.iter().map(|byte| format!("{byte:02x}")).collect()
}

impl SsoLoginState {
    /// Mint a fresh outstanding login.
    pub fn new(now: DateTime<Utc>) -> Self {
        Self {
            state: random_hex(32),
            nonce: random_hex(16),
            pkce_verifier: random_hex(48),
            expires_at: now + Duration::seconds(STATE_TTL_SECS),
        }
    }

    /// Validate a callback against this outstanding state. Pure.
    pub fn validate_callback(
        &self,
        state_got: &str,
        nonce_got: &str,
        now: DateTime<Utc>,
    ) -> Result<(), SsoError> {
        use subtle_constant_time_eq::ct_eq;
        self.validate_state(state_got, now)?;
        if !ct_eq(self.nonce.as_bytes(), nonce_got.as_bytes()) {
            return Err(SsoError::NonceMismatch);
        }
        Ok(())
    }

    /// Validate the state binding and expiry only. The nonce check is
    /// delegated to the OIDC port, which compares the ID-token claim
    /// against this outstanding login's nonce during code exchange.
    pub fn validate_state(&self, state_got: &str, now: DateTime<Utc>) -> Result<(), SsoError> {
        use subtle_constant_time_eq::ct_eq;
        if !ct_eq(self.state.as_bytes(), state_got.as_bytes()) {
            return Err(SsoError::StateMismatch);
        }
        if now >= self.expires_at {
            return Err(SsoError::StateExpired);
        }
        Ok(())
    }
}

// Minimal constant-time comparison to avoid adding a dependency; the
// lengths differ for nearly all mismatched inputs anyway, which is
// not secret material.
mod subtle_constant_time_eq {
    pub fn ct_eq(a: &[u8], b: &[u8]) -> bool {
        if a.len() != b.len() {
            return false;
        }
        let mut diff = 0u8;
        for (left, right) in a.iter().zip(b.iter()) {
            diff |= left ^ right;
        }
        diff == 0
    }
}

/// Claims extracted from a verified ID token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OidcClaims {
    /// The token's `iss` claim.
    pub issuer: String,
    /// The token's stable `sub` claim.
    pub subject: String,
    /// Optional email claim used when provisioning.
    pub email: Option<String>,
}

/// Port wrapping the OIDC protocol (discovery, authorization URL,
/// code exchange). Implemented in the app layer over `openidconnect`.
#[async_trait::async_trait]
pub trait OidcPort: Send + Sync + 'static {
    /// Build the provider authorization URL for one outstanding login.
    async fn authorize_url(
        &self,
        connection: &SsoConnection,
        state: &SsoLoginState,
    ) -> Result<String, SsoError>;

    /// Exchange `code` for verified claims.
    async fn exchange(
        &self,
        connection: &SsoConnection,
        code: &str,
        pkce_verifier: &str,
        expected_nonce: &str,
    ) -> Result<OidcClaims, SsoError>;
}

/// Persistence port for connections, identity links, and login states.
#[async_trait::async_trait]
pub trait SsoRepository: Send + Sync + 'static {
    /// Load the singleton connection, if configured.
    async fn get_connection(&self) -> Result<Option<SsoConnection>, RepoError>;
    /// Replace the singleton connection.
    async fn save_connection(&self, connection: &SsoConnection) -> Result<(), RepoError>;

    /// Find a linked user for (issuer, subject).
    async fn find_identity(
        &self,
        issuer: &str,
        subject: &str,
    ) -> Result<Option<ExternalIdentity>, RepoError>;
    /// Store an identity link.
    async fn save_identity(&self, identity: &ExternalIdentity) -> Result<(), RepoError>;

    /// Store an outstanding login state.
    async fn insert_state(&self, state: &SsoLoginState) -> Result<(), RepoError>;
    /// Take (load and delete) an outstanding login state.
    async fn take_state(&self, state: &str) -> Result<Option<SsoLoginState>, RepoError>;
    /// Delete states older than `cutoff`; returns rows removed.
    async fn purge_states(&self, cutoff: DateTime<Utc>) -> Result<u64, RepoError>;
}

#[cfg(test)]
mod tests;
