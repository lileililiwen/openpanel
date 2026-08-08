use std::fmt;

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Duration, Utc};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::identity::role::Role;

const SESSION_TOKEN_BYTES: usize = 32;
/// Maximum inactivity allowed before a session expires.
pub const SESSION_INACTIVITY: Duration = Duration::hours(24);
/// Absolute lifetime of a session.
pub const SESSION_ABSOLUTE: Duration = Duration::days(7);

/// Errors related to session token parsing.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum SessionTokenError {
    /// The token has an invalid length.
    #[error("session token length invalid")]
    Length,
    /// The token is not valid base64url.
    #[error("session token is not valid base64url")]
    Encoding,
}

/// A random, opaque session token.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionToken(String);

impl SessionToken {
    /// Generate a fresh 256-bit token.
    pub fn generate() -> Self {
        let mut buf = [0u8; SESSION_TOKEN_BYTES];
        rand::rngs::OsRng.fill_bytes(&mut buf);
        Self(URL_SAFE_NO_PAD.encode(buf))
    }

    /// Parse a token from a base64url string, validating its length.
    pub fn from_string(s: impl Into<String>) -> Result<Self, SessionTokenError> {
        let s = s.into();
        let bytes = URL_SAFE_NO_PAD
            .decode(&s)
            .map_err(|_| SessionTokenError::Encoding)?;
        if bytes.len() != SESSION_TOKEN_BYTES {
            return Err(SessionTokenError::Length);
        }
        Ok(Self(s))
    }

    /// Return the token's underlying string.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SessionToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A user session with an expiry policy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    /// The session's unique identifier.
    pub id: Uuid,
    /// The owning user's identifier.
    pub user_id: Uuid,
    /// The hashed session token.
    pub token_hash: String,
    /// The role the session grants.
    pub role: Role,
    /// When the session was created.
    pub created_at: DateTime<Utc>,
    /// When the session was last active.
    pub last_seen_at: DateTime<Utc>,
    /// When the session absolutely expires.
    pub absolute_expires_at: DateTime<Utc>,
    /// The IP address the session originated from.
    pub source_ip: Option<String>,
    /// The user agent of the session client.
    pub user_agent: Option<String>,
}

impl Session {
    /// Create a new session, returning it alongside its plaintext token.
    pub fn new(
        user_id: Uuid,
        role: Role,
        token: &SessionToken,
        source_ip: Option<String>,
        user_agent: Option<String>,
    ) -> (Self, SessionToken) {
        use argon2::{
            Argon2,
            password_hash::{PasswordHasher, SaltString, rand_core::OsRng},
        };

        let salt = SaltString::generate(&mut OsRng);
        #[allow(clippy::expect_used)]
        let hash = Argon2::default()
            .hash_password(token.expose().as_bytes(), &salt)
            .expect("argon2 params are compile-time constants; hashing with default params is infallible")
            .to_string();

        let now = Utc::now();
        let session = Self {
            id: Uuid::new_v4(),
            user_id,
            token_hash: hash,
            role,
            created_at: now,
            last_seen_at: now,
            absolute_expires_at: now + SESSION_ABSOLUTE,
            source_ip,
            user_agent,
        };
        (session, token.clone())
    }

    /// Mark the session as active as of now.
    pub fn touch(&mut self) {
        self.last_seen_at = Utc::now();
    }

    /// Whether the session has expired relative to the given time.
    pub fn is_expired(&self, now: DateTime<Utc>) -> bool {
        now > self.absolute_expires_at || now - self.last_seen_at > SESSION_INACTIVITY
    }

    /// Verify a plaintext token against this session's hash.
    pub fn verify_token(&self, token: &SessionToken) -> bool {
        use argon2::{
            Argon2,
            password_hash::{PasswordHash, PasswordVerifier},
        };
        let parsed = match PasswordHash::new(&self.token_hash) {
            Ok(p) => p,
            Err(_) => return false,
        };
        Argon2::default()
            .verify_password(token.expose().as_bytes(), &parsed)
            .is_ok()
    }

    /// Return the session's identifier.
    pub fn id(&self) -> Uuid {
        self.id
    }

    /// Return the owning user's identifier.
    pub fn user_id(&self) -> Uuid {
        self.user_id
    }

    /// Return the session's role.
    pub fn role(&self) -> Role {
        self.role
    }

    /// Return when the session was created.
    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    /// Return when the session was last active.
    pub fn last_seen_at(&self) -> DateTime<Utc> {
        self.last_seen_at
    }

    /// Return when the session absolutely expires.
    pub fn absolute_expires_at(&self) -> DateTime<Utc> {
        self.absolute_expires_at
    }

    /// Return the originating IP address, if any.
    pub fn source_ip(&self) -> Option<&str> {
        self.source_ip.as_deref()
    }

    /// Return the user agent, if any.
    pub fn user_agent(&self) -> Option<&str> {
        self.user_agent.as_deref()
    }

    /// Return the session's token hash.
    pub fn token_hash(&self) -> &str {
        &self.token_hash
    }
}

/// Builder returned by `Session::restore`. Finalised by `.build()`.
pub struct SessionBuilder {
    /// The session's unique identifier.
    pub id: Uuid,
    /// The owning user's identifier.
    pub user_id: Uuid,
    /// The hashed session token.
    pub token_hash: String,
    /// The role the session grants.
    pub role: Role,
    /// When the session was created.
    pub created_at: DateTime<Utc>,
    /// When the session was last active.
    pub last_seen_at: DateTime<Utc>,
    /// When the session absolutely expires.
    pub absolute_expires_at: DateTime<Utc>,
    /// The IP address the session originated from.
    pub source_ip: Option<String>,
    /// The user agent of the session client.
    pub user_agent: Option<String>,
}

impl SessionBuilder {
    /// Consume the builder and produce the session.
    pub fn build(self) -> Session {
        Session {
            id: self.id,
            user_id: self.user_id,
            token_hash: self.token_hash,
            role: self.role,
            created_at: self.created_at,
            last_seen_at: self.last_seen_at,
            absolute_expires_at: self.absolute_expires_at,
            source_ip: self.source_ip,
            user_agent: self.user_agent,
        }
    }
}

impl Session {
    /// Restore a session from persistence. Used by repository adapters.
    pub fn restore(builder: SessionBuilder) -> Session {
        builder.build()
    }
}
