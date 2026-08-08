use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// The lifecycle status of a database.
pub enum DatabaseStatus {
    /// The database is provisioned and usable.
    Active,
    /// The database is suspended and cannot be used.
    Suspended,
}

impl DatabaseStatus {
    /// The string representation of the status, as persisted.
    pub fn as_str(&self) -> &'static str {
        match self {
            DatabaseStatus::Active => "active",
            DatabaseStatus::Suspended => "suspended",
        }
    }
}

impl fmt::Display for DatabaseStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for DatabaseStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "active" => Ok(DatabaseStatus::Active),
            "suspended" => Ok(DatabaseStatus::Suspended),
            other => Err(format!("unknown status `{other}`")),
        }
    }
}
