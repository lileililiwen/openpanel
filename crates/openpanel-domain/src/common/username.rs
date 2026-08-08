//! Validated username value object.

use std::fmt;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Errors that can occur when constructing a username.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum UsernameError {
    /// Username is outside the allowed length range.
    #[error("username must be 3-32 chars long")]
    Length,
    /// Username contains a character that is not allowed.
    #[error("username contains invalid character `{0}`")]
    InvalidChar(char),
}

/// Validated username value object.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Username(String);

impl Username {
    /// Create a new `Username`, validating the value.
    pub fn new(value: impl Into<String>) -> Result<Self, UsernameError> {
        let s = value.into();
        validate(&s)?;
        Ok(Self(s))
    }

    /// Return the raw username string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Username {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

fn validate(s: &str) -> Result<(), UsernameError> {
    if s.len() < 3 || s.len() > 32 {
        return Err(UsernameError::Length);
    }
    for c in s.chars() {
        let ok = c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.';
        if !ok {
            return Err(UsernameError::InvalidChar(c));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_valid_usernames() {
        assert!(Username::new("alice").is_ok());
        assert!(Username::new("admin_user").is_ok());
        assert!(Username::new("first.last").is_ok());
    }

    #[test]
    fn rejects_invalid_usernames() {
        assert_eq!(Username::new("ab"), Err(UsernameError::Length));
        assert_eq!(Username::new("a".repeat(33)), Err(UsernameError::Length));
        assert!(matches!(
            Username::new("has space"),
            Err(UsernameError::InvalidChar(' '))
        ));
    }
}
