use async_trait::async_trait;
use uuid::Uuid;

use crate::databases::database::Database;
use crate::databases::status::DatabaseStatus;
use crate::RepoError;

#[async_trait]
pub trait DatabaseRepository: Send + Sync + 'static {
    async fn insert(&self, db: &Database, password_ciphertext: &str) -> Result<(), RepoError>;
    async fn find_by_id(&self, id: Uuid) -> Result<Option<Database>, RepoError>;
    async fn find_by_name(&self, name: &str) -> Result<Option<Database>, RepoError>;
    async fn list_all(&self) -> Result<Vec<Database>, RepoError>;
    async fn list_by_owner(&self, owner_id: Uuid) -> Result<Vec<Database>, RepoError>;
    async fn password_ciphertext(&self, id: Uuid) -> Result<Option<String>, RepoError>;
    async fn update_password_ciphertext(&self, id: Uuid, ciphertext: &str)
        -> Result<(), RepoError>;
    async fn update_status(&self, id: Uuid, status: DatabaseStatus) -> Result<(), RepoError>;
    async fn delete(&self, id: Uuid) -> Result<(), RepoError>;
    async fn count(&self) -> Result<i64, RepoError>;
}