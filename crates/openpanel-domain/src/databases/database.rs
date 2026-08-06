use chrono::{DateTime, Utc};
use regex::Regex;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::databases::engine::DatabaseEngine;
use crate::databases::error::DatabaseError;
use crate::databases::status::DatabaseStatus;

const NAME_RE: &str = r"^[a-z0-9_]{3,32}$";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Database {
    id: Uuid,
    owner_id: Uuid,
    name: String,
    db_user: String,
    db_host: String,
    engine: DatabaseEngine,
    charset: String,
    status: DatabaseStatus,
    created_at: DateTime<Utc>,
    created_by: String,
}

impl Database {
    pub fn new(
        id: Uuid,
        owner_id: Uuid,
        owner_username: &str,
        suffix: &str,
        charset: impl Into<String>,
        created_by: impl Into<String>,
    ) -> Result<Self, DatabaseError> {
        validate_name(suffix)?;
        let charset = charset.into();
        if charset.is_empty() {
            return Err(DatabaseError::InvalidCharset("empty".into()));
        }

        let prefix = validate_username(owner_username)?;
        let name = format!("{prefix}_{suffix}");
        let db_user = name.clone();

        Ok(Self {
            id,
            owner_id,
            name,
            db_user,
            db_host: "localhost".to_string(),
            engine: DatabaseEngine::Mysql,
            charset,
            status: DatabaseStatus::Active,
            created_at: Utc::now(),
            created_by: created_by.into(),
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn restore(
        id: Uuid,
        owner_id: Uuid,
        name: String,
        db_user: String,
        db_host: String,
        engine: DatabaseEngine,
        charset: String,
        status: DatabaseStatus,
        created_at: DateTime<Utc>,
        created_by: String,
    ) -> Self {
        Self {
            id,
            owner_id,
            name,
            db_user,
            db_host,
            engine,
            charset,
            status,
            created_at,
            created_by,
        }
    }

    pub fn suspend(&mut self) {
        self.status = DatabaseStatus::Suspended;
    }
    pub fn resume(&mut self) {
        self.status = DatabaseStatus::Active;
    }

    pub fn id(&self) -> Uuid {
        self.id
    }
    pub fn owner_id(&self) -> Uuid {
        self.owner_id
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn db_user(&self) -> &str {
        &self.db_user
    }
    pub fn db_host(&self) -> &str {
        &self.db_host
    }
    pub fn engine(&self) -> DatabaseEngine {
        self.engine
    }
    pub fn charset(&self) -> &str {
        &self.charset
    }
    pub fn status(&self) -> DatabaseStatus {
        self.status
    }
    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    pub fn created_by(&self) -> &str {
        &self.created_by
    }
}

fn validate_name(s: &str) -> Result<(), DatabaseError> {
    let re = Regex::new(NAME_RE).expect("valid regex");
    if !re.is_match(s) {
        return Err(DatabaseError::InvalidName(format!(
            "`{s}` does not match {NAME_RE}"
        )));
    }
    Ok(())
}

fn validate_username(s: &str) -> Result<String, DatabaseError> {
    let re = Regex::new(NAME_RE).expect("valid regex");
    if !re.is_match(s) {
        return Err(DatabaseError::InvalidName(format!(
            "owner username `{s}` is not a valid name component"
        )));
    }
    Ok(s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_with_owner_prefix() {
        let d = Database::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            "alice",
            "app",
            "utf8mb4",
            "tester",
        )
        .unwrap();
        assert_eq!(d.name(), "alice_app");
        assert_eq!(d.db_user(), "alice_app");
        assert_eq!(d.engine(), DatabaseEngine::Mysql);
    }

    #[test]
    fn rejects_bad_suffix() {
        let r = Database::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            "alice",
            "ab",
            "utf8mb4",
            "t",
        );
        assert!(matches!(r, Err(DatabaseError::InvalidName(_))));
    }

    #[test]
    fn rejects_bad_owner_username() {
        let r = Database::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            "bad user",
            "app",
            "utf8mb4",
            "t",
        );
        assert!(matches!(r, Err(DatabaseError::InvalidName(_))));
    }

    #[test]
    fn suspend_resume() {
        let mut d = Database::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            "alice",
            "app",
            "utf8mb4",
            "t",
        )
        .unwrap();
        assert_eq!(d.status(), DatabaseStatus::Active);
        d.suspend();
        assert_eq!(d.status(), DatabaseStatus::Suspended);
        d.resume();
        assert_eq!(d.status(), DatabaseStatus::Active);
    }
}