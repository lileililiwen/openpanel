use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Owner,
    Admin,
    User,
}

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
    pub fn as_str(&self) -> &'static str {
        match self {
            Role::Owner => "owner",
            Role::Admin => "admin",
            Role::User => "user",
        }
    }

    pub fn can_manage_users(&self) -> bool {
        matches!(self, Role::Owner)
    }

    pub fn can_change_roles(&self) -> bool {
        matches!(self, Role::Owner)
    }

    pub fn can_manage_sites(&self) -> bool {
        matches!(self, Role::Owner | Role::Admin)
    }

    pub fn can_manage_databases(&self) -> bool {
        matches!(self, Role::Owner | Role::Admin)
    }

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