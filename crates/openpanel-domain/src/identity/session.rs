use std::fmt;

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chrono::{DateTime, Duration, Utc};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::identity::role::Role;

const SESSION_TOKEN_BYTES: usize = 32;
pub const SESSION_INACTIVITY: Duration = Duration::hours(24);
pub const SESSION_ABSOLUTE: Duration = Duration::days(7);

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum SessionTokenError {
    #[error("session token length invalid")]
    Length,
    #[error("session token is not valid base64url")]
    Encoding,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionToken(String);

impl SessionToken {
    /// Generate a fresh 256-bit token.
    pub fn generate() -> Self {
        let mut buf = [0u8; SESSION_TOKEN_BYTES];
        rand::rngs::OsRng.fill_bytes(&mut buf);
        Self(URL_SAFE_NO_PAD.encode(buf))
    }

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

    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SessionToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: Uuid,
    pub user_id: Uuid,
    pub token_hash: String,
    pub role: Role,
    pub created_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
    pub absolute_expires_at: DateTime<Utc>,
    pub source_ip: Option<String>,
    pub user_agent: Option<String>,
}

impl Session {
    pub fn new(
        user_id: Uuid,
        role: Role,
        token: &SessionToken,
        source_ip: Option<String>,
        user_agent: Option<String>,
    ) -> (Self, SessionToken) {
        use argon2::password_hash::rand_core::OsRng;
        use argon2::password_hash::{PasswordHasher, SaltString};
        use argon2::Argon2;

        let salt = SaltString::generate(&mut OsRng);
        let hash = Argon2::default()
            .hash_password(token.expose().as_bytes(), &salt)
            .expect("argon2 hash")
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

    pub fn touch(&mut self) {
        self.last_seen_at = Utc::now();
    }

    pub fn is_expired(&self, now: DateTime<Utc>) -> bool {
        now > self.absolute_expires_at
            || now - self.last_seen_at > SESSION_INACTIVITY
    }

    pub fn verify_token(&self, token: &SessionToken) -> bool {
        use argon2::password_hash::{PasswordHash, PasswordVerifier};
        use argon2::Argon2;
        let parsed = match PasswordHash::new(&self.token_hash) {
            Ok(p) => p,
            Err(_) => return false,
        };
        Argon2::default()
            .verify_password(token.expose().as_bytes(), &parsed)
            .is_ok()
    }

    pub fn id(&self) -> Uuid {
        self.id
    }
    pub fn user_id(&self) -> Uuid {
        self.user_id
    }
    pub fn role(&self) -> Role {
        self.role
    }
    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    pub fn last_seen_at(&self) -> DateTime<Utc> {
        self.last_seen_at
    }
    pub fn absolute_expires_at(&self) -> DateTime<Utc> {
        self.absolute_expires_at
    }
    pub fn source_ip(&self) -> Option<&str> {
        self.source_ip.as_deref()
    }
    pub fn user_agent(&self) -> Option<&str> {
        self.user_agent.as_deref()
    }
    pub fn token_hash(&self) -> &str {
        &self.token_hash
    }
}

/// Builder returned by `Session::restore`. Finalised by `.build()`.
pub struct SessionBuilder {
    pub id: Uuid,
    pub user_id: Uuid,
    pub token_hash: String,
    pub role: Role,
    pub created_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
    pub absolute_expires_at: DateTime<Utc>,
    pub source_ip: Option<String>,
    pub user_agent: Option<String>,
}

impl SessionBuilder {
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