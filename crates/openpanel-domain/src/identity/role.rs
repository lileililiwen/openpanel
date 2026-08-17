use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// A user's role within the system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    /// Full ownership of the account.
    Owner,
    /// Administrative access short of ownership.
    Admin,
    /// A regular user.
    User,
}

/// Error returned when parsing an unknown role string.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
#[error("unknown role `{0}`")]
pub struct RoleParseError(pub String);

impl FromStr for Role {
    type Err = RoleParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "owner" => Ok(Role::Owner),
            "admin" => Ok(Role::Admin),
            "user" => Ok(Role::User),
            other => Err(RoleParseError(other.to_string())),
        }
    }
}

impl Role {
    /// Return the snake_case string form of the role.
    pub fn as_str(&self) -> &'static str {
        match self {
            Role::Owner => "owner",
            Role::Admin => "admin",
            Role::User => "user",
        }
    }

    /// Whether the role may manage users.
    pub fn can_manage_users(&self) -> bool {
        matches!(self, Role::Owner)
    }

    /// Whether the role may change user roles.
    pub fn can_change_roles(&self) -> bool {
        matches!(self, Role::Owner)
    }

    /// Whether the role may manage sites.
    pub fn can_manage_sites(&self) -> bool {
        matches!(self, Role::Owner | Role::Admin)
    }

    /// Whether the role may manage databases.
    pub fn can_manage_databases(&self) -> bool {
        matches!(self, Role::Owner | Role::Admin)
    }

    /// Whether the role may manage files.
    pub fn can_manage_files(&self) -> bool {
        matches!(self, Role::Owner | Role::Admin)
    }
}

impl fmt::Display for Role {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permissions() {
        assert!(Role::Owner.can_manage_users());
        assert!(!Role::Admin.can_manage_users());
        assert!(!Role::User.can_manage_users());

        assert!(Role::Owner.can_change_roles());
        assert!(!Role::Admin.can_change_roles());

        assert!(Role::Admin.can_manage_sites());
        assert!(!Role::User.can_manage_sites());
    }
}
