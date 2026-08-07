use chrono::{DateTime, Utc};
use openpanel_domain::{Database, DatabaseEngine, DatabaseStatus};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct CreateDatabaseRequest {
    pub owner_id: Option<Uuid>,
    pub owner_username: String,
    pub suffix: String,
    #[serde(default)]
    pub charset: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct DatabaseDto {
    pub id: Uuid,
    pub owner_id: Uuid,
    pub name: String,
    pub db_user: String,
    pub db_host: String,
    pub engine: DatabaseEngine,
    pub charset: String,
    pub status: DatabaseStatus,
    pub created_at: DateTime<Utc>,
    pub created_by: String,
}

impl DatabaseDto {
    pub fn from_database(db: &Database) -> Self {
        Self {
            id: db.id(),
            owner_id: db.owner_id(),
            name: db.name().to_string(),
            db_user: db.db_user().to_string(),
            db_host: db.db_host().to_string(),
            engine: db.engine(),
            charset: db.charset().to_string(),
            status: db.status(),
            created_at: db.created_at(),
            created_by: db.created_by().to_string(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct CreatedDatabaseResponse {
    #[serde(flatten)]
    pub database: DatabaseDto,
    /// Plaintext password, returned ONCE on create or rotate. Never returned
    /// by GET /databases/{id}.
    pub password: String,
}
