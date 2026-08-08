use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// The database engine used by a database.
pub enum DatabaseEngine {
    /// MySQL 8.x, provisioned through the `mysql` CLI.
    Mysql,
}

impl DatabaseEngine {
    /// The string representation of the engine, as used in MySQL commands.
    pub fn as_str(&self) -> &'static str {
        match self {
            DatabaseEngine::Mysql => "mysql",
        }
    }
}

impl fmt::Display for DatabaseEngine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for DatabaseEngine {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "mysql" => Ok(DatabaseEngine::Mysql),
            other => Err(format!("unknown database engine `{other}`")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        assert_eq!(DatabaseEngine::Mysql.as_str(), "mysql");
        assert_eq!(
            "mysql".parse::<DatabaseEngine>().unwrap(),
            DatabaseEngine::Mysql
        );
    }
}
