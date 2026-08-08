//! Validated email address value object.

use std::fmt;

use serde::{Deserialize, Serialize};

use super::error::DomainError;

/// Validated email address. RFC 5321 compatible (simplified).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Email(String);

impl Email {
    /// Create a new `Email`, validating the address.
    pub fn new(value: impl Into<String>) -> Result<Self, DomainError> {
        let s = value.into();
        validate(&s)?;
        Ok(Self(s))
    }

    /// Return the raw email address string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Email {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for Email {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

fn validate(s: &str) -> Result<(), DomainError> {
    if s.is_empty() || s.len() > 254 {
        return Err(DomainError::Validation("email length out of range".into()));
    }
    let at = match s.find('@') {
        Some(i) => i,
        None => return Err(DomainError::Validation("email missing '@'".into())),
    };
    if at == 0 || at == s.len() - 1 {
        return Err(DomainError::Validation(
            "email has empty local/domain part".into(),
        ));
    }
    let domain = &s[at + 1..];
    if !domain.contains('.') {
        return Err(DomainError::Validation(
            "email domain must contain a dot".into(),
        ));
    }
    if domain.starts_with('.') || domain.ends_with('.') {
        return Err(DomainError::Validation(
            "email domain cannot start or end with a dot".into(),
        ));
    }
    if !s.chars().all(|c| c.is_ascii_graphic() || c == ' ') {
        return Err(DomainError::Validation(
            "email contains invalid characters".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_valid_emails() {
        assert!(Email::new("user@example.com").is_ok());
        assert!(Email::new("first.last+tag@example.co.uk").is_ok());
    }

    #[test]
    fn rejects_invalid_emails() {
        assert!(Email::new("").is_err());
        assert!(Email::new("no-at-sign").is_err());
        assert!(Email::new("@example.com").is_err());
        assert!(Email::new("user@").is_err());
        assert!(Email::new("user@nodomain").is_err());
    }
}
