use async_trait::async_trait;
use uuid::Uuid;

use crate::{
    RepoError,
    databases::{database::Database, status::DatabaseStatus},
};

/// Persistence contract for the `Database` aggregate. Implementations
/// own all MySQL and encryption I/O; this trait expresses the data
/// operations the domain requires.
#[async_trait]
pub trait DatabaseRepository: Send + Sync + 'static {
    /// Persist a new database along with its encrypted password.
    async fn insert(&self, db: &Database, password_ciphertext: &str) -> Result<(), RepoError>;

    /// Look up a database by its unique identifier.
    async fn find_by_id(&self, id: Uuid) -> Result<Option<Database>, RepoError>;

    /// Look up a database by its fully qualified name.
    async fn find_by_name(&self, name: &str) -> Result<Option<Database>, RepoError>;

    /// List every database.
    async fn list_all(&self) -> Result<Vec<Database>, RepoError>;

    /// List the databases owned by a user.
    async fn list_by_owner(&self, owner_id: Uuid) -> Result<Vec<Database>, RepoError>;

    /// Read the stored encrypted password for a database.
    async fn password_ciphertext(&self, id: Uuid) -> Result<Option<String>, RepoError>;

    /// Replace the stored encrypted password for a database.
    async fn update_password_ciphertext(&self, id: Uuid, ciphertext: &str)
    -> Result<(), RepoError>;

    /// Update the lifecycle status of a database.
    async fn update_status(&self, id: Uuid, status: DatabaseStatus) -> Result<(), RepoError>;

    /// Remove a database.
    async fn delete(&self, id: Uuid) -> Result<(), RepoError>;

    /// Count the total number of databases.
    async fn count(&self) -> Result<i64, RepoError>;
}
