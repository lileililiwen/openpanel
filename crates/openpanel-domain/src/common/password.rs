//! Password value object. Constructed from plaintext; immediately hashed;
//! plaintext is never retained.

use std::fmt;

use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use serde::{Deserialize, Serialize};
use thiserror::Error;

const MIN_PASSWORD_LEN: usize = 12;
const ARGON2_M_COST: u32 = 19456;
const ARGON2_T_COST: u32 = 2;
const ARGON2_P_COST: u32 = 1;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum PasswordError {
    #[error("password must be at least {0} characters")]
    TooShort(usize),
    #[error("password exceeds maximum length")]
    TooLong,
    #[error("argon2 hash error: {0}")]
    Hash(String),
    #[error("argon2 verify error: {0}")]
    Verify(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Password {
    /// argon2id PHC-format string. Plaintext is dropped at construction.
    hash: String,
}

impl Password {
    /// Hash a plaintext password. The plaintext is not retained.
    pub fn hash(plaintext: &str) -> Result<Self, PasswordError> {
        if plaintext.len() < MIN_PASSWORD_LEN {
            return Err(PasswordError::TooShort(MIN_PASSWORD_LEN));
        }
        if plaintext.len() > 1024 {
            return Err(PasswordError::TooLong);
        }

        let salt = SaltString::generate(&mut OsRng);
        let argon = Argon2::new(
            argon2::Algorithm::Argon2id,
            argon2::Version::V0x13,
            argon2::Params::new(ARGON2_M_COST, ARGON2_T_COST, ARGON2_P_COST, None)
                .map_err(|e| PasswordError::Hash(e.to_string()))?,
        );
        let hash = argon
            .hash_password(plaintext.as_bytes(), &salt)
            .map_err(|e| PasswordError::Hash(e.to_string()))?
            .to_string();
        Ok(Self { hash })
    }

    pub fn from_hash(hash: impl Into<String>) -> Self {
        Self { hash: hash.into() }
    }

    pub fn verify(&self, plaintext: &str) -> Result<bool, PasswordError> {
        let parsed = PasswordHash::new(&self.hash)
            .map_err(|e| PasswordError::Verify(e.to_string()))?;
        Ok(Argon2::default()
            .verify_password(plaintext.as_bytes(), &parsed)
            .is_ok())
    }

    pub fn hash_str(&self) -> &str {
        &self.hash
    }
}

impl fmt::Display for Password {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Password(redacted)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_short_passwords() {
        assert!(matches!(
            Password::hash("short"),
            Err(PasswordError::TooShort(12))
        ));
    }

    #[test]
    fn hashes_and_verifies() {
        let p = Password::hash("correct horse battery staple").unwrap();
        assert!(p.verify("correct horse battery staple").unwrap());
        assert!(!p.verify("wrong horse").unwrap());
    }
}