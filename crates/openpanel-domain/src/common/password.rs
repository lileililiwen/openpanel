//! Password value object. Constructed from plaintext; immediately hashed;
//! plaintext is never retained.

use std::{
    fmt,
    sync::atomic::{AtomicU32, Ordering},
};

use argon2::{
    Argon2,
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString, rand_core::OsRng},
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

const MIN_PASSWORD_LEN: usize = 12;
const ARGON2_M_COST: u32 = 19456;
const ARGON2_T_COST: u32 = 2;
const ARGON2_P_COST: u32 = 1;

/// Argon2 memory cost (KiB), overridable for test/seed fixtures.
static ARGON2_M_COST_OVERRIDE: AtomicU32 = AtomicU32::new(ARGON2_M_COST);
/// Argon2 time cost, overridable for test/seed fixtures.
static ARGON2_T_COST_OVERRIDE: AtomicU32 = AtomicU32::new(ARGON2_T_COST);

/// Errors that can occur while hashing or verifying a password.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum PasswordError {
    /// Password is shorter than the minimum allowed length.
    #[error("password must be at least {0} characters")]
    TooShort(usize),
    /// Password exceeds the maximum allowed length.
    #[error("password exceeds maximum length")]
    TooLong,
    /// The argon2 hashing operation failed.
    #[error("argon2 hash error: {0}")]
    Hash(String),
    /// The argon2 verification operation failed.
    #[error("argon2 verify error: {0}")]
    Verify(String),
}

/// Hashed password value object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Password {
    /// argon2id PHC-format string. Plaintext is dropped at construction.
    hash: String,
}

impl Password {
    /// Lower the argon2 costs for test fixtures and seed data.
    ///
    /// The resulting hash is cryptographically weak and MUST NOT be used
    /// for real account secrets. `openpanel-test-support` sets this for
    /// every test database; production binaries never call it, so the
    /// production default (`19456` KiB / `2` iterations) is unchanged.
    pub fn set_test_costs(m_cost: u32, t_cost: u32) {
        ARGON2_M_COST_OVERRIDE.store(m_cost.max(8), Ordering::Relaxed);
        ARGON2_T_COST_OVERRIDE.store(t_cost.max(1), Ordering::Relaxed);
    }

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
            argon2::Params::new(
                ARGON2_M_COST_OVERRIDE.load(Ordering::Relaxed),
                ARGON2_T_COST_OVERRIDE.load(Ordering::Relaxed),
                ARGON2_P_COST,
                None,
            )
            .map_err(|e| PasswordError::Hash(e.to_string()))?,
        );
        let hash = argon
            .hash_password(plaintext.as_bytes(), &salt)
            .map_err(|e| PasswordError::Hash(e.to_string()))?
            .to_string();
        Ok(Self { hash })
    }

    /// Construct a `Password` from an existing argon2id PHC-format hash string.
    pub fn from_hash(hash: impl Into<String>) -> Self {
        Self { hash: hash.into() }
    }

    /// Verify a plaintext candidate against the stored hash.
    pub fn verify(&self, plaintext: &str) -> Result<bool, PasswordError> {
        let parsed =
            PasswordHash::new(&self.hash).map_err(|e| PasswordError::Verify(e.to_string()))?;
        Ok(Argon2::default()
            .verify_password(plaintext.as_bytes(), &parsed)
            .is_ok())
    }

    /// Return the stored argon2id hash string.
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
        Password::set_test_costs(8, 1);
        let p = Password::hash("correct horse battery staple").unwrap();
        assert!(p.verify("correct horse battery staple").unwrap());
        assert!(!p.verify("wrong horse").unwrap());
    }
}

#[cfg(test)]
mod prop {
    use proptest::prelude::*;

    use super::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(10))]

        #[test]
        fn prop_hash_and_verify_roundtrip(p in "[a-zA-Z0-9 !@#$%^&*]{12,256}") {
            Password::set_test_costs(8, 1);
            let pw = Password::hash(&p).unwrap();
            prop_assert!(pw.verify(&p).unwrap());
        }

        #[test]
        fn prop_short_password_rejected(p in "[a-zA-Z0-9]{1,11}") {
            prop_assert!(matches!(
                Password::hash(&p),
                Err(PasswordError::TooShort(MIN_PASSWORD_LEN))
            ));
        }
    }
}
