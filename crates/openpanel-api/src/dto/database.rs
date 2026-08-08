use chrono::{DateTime, Utc};
use openpanel_domain::{Database, DatabaseEngine, DatabaseStatus};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Request body for `POST /databases`.
#[derive(Debug, Deserialize)]
pub struct CreateDatabaseRequest {
    /// Optional explicit owner; defaults to the caller when omitted.
    pub owner_id: Option<Uuid>,
    /// Logical owner name (used to derive connection credentials / paths).
    pub owner_username: String,
    /// Per-database suffix appended to the generated database name.
    pub suffix: String,
    /// MySQL charset (e.g. `utf8mb4`); when `None` a sensible default is used.
    #[serde(default)]
    pub charset: Option<String>,
}

/// Public-facing projection of a [`Database`] returned to clients.
#[derive(Debug, Serialize)]
pub struct DatabaseDto {
    /// Unique database identifier.
    pub id: Uuid,
    /// Owning user's id.
    pub owner_id: Uuid,
    /// Fully-qualified database name as served by the engine.
    pub name: String,
    /// Database account username used by clients to connect.
    pub db_user: String,
    /// Hostname the database is reachable at.
    pub db_host: String,
    /// Storage engine backing this database.
    pub engine: DatabaseEngine,
    /// Active character set.
    pub charset: String,
    /// Current lifecycle state.
    pub status: DatabaseStatus,
    /// Timestamp the database was provisioned.
    pub created_at: DateTime<Utc>,
    /// Username of the principal who provisioned the database.
    pub created_by: String,
}

impl DatabaseDto {
    /// Projects a domain [`Database`] into its wire DTO form.
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

/// Response body for successful database creation or password rotation.
#[derive(Debug, Serialize)]
pub struct CreatedDatabaseResponse {
    /// The newly created / rotated database, flattened into the response.
    #[serde(flatten)]
    pub database: DatabaseDto,
    /// Plaintext password, returned ONCE on create or rotate. Never returned
    /// by GET /databases/{id}.
    pub password: String,
}
