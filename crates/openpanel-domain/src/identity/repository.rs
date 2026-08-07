use async_trait::async_trait;
use uuid::Uuid;

use crate::RepoError;
use crate::identity::role::Role;
use crate::identity::session::{Session, SessionToken};
use crate::identity::user::User;

#[async_trait]
pub trait UserRepository: Send + Sync + 'static {
    async fn insert(&self, user: &User) -> Result<(), RepoError>;
    async fn find_by_id(&self, id: Uuid) -> Result<Option<User>, RepoError>;
    async fn find_by_username(&self, username: &str) -> Result<Option<User>, RepoError>;
    async fn find_by_email(&self, email: &str) -> Result<Option<User>, RepoError>;
    async fn list(&self) -> Result<Vec<User>, RepoError>;
    async fn update_role(&self, id: Uuid, role: Role) -> Result<(), RepoError>;
    async fn disable(&self, id: Uuid) -> Result<(), RepoError>;
    async fn update_last_login(&self, id: Uuid) -> Result<(), RepoError>;
    async fn update_password(&self, id: Uuid, hash: &str) -> Result<(), RepoError>;
    async fn delete(&self, id: Uuid) -> Result<(), RepoError>;
    async fn count(&self) -> Result<i64, RepoError>;
}

#[async_trait]
pub trait SessionRepository: Send + Sync + 'static {
    async fn insert(&self, session: &Session) -> Result<(), RepoError>;
    async fn find_by_token_hash_match(
        &self,
        token: &SessionToken,
    ) -> Result<Option<Session>, RepoError>;
    async fn find_by_id(&self, id: Uuid) -> Result<Option<Session>, RepoError>;
    async fn touch(&self, id: Uuid) -> Result<(), RepoError>;
    async fn delete(&self, id: Uuid) -> Result<(), RepoError>;
    async fn delete_for_user(&self, user_id: Uuid) -> Result<(), RepoError>;
    async fn purge_expired(&self) -> Result<u64, RepoError>;
}
