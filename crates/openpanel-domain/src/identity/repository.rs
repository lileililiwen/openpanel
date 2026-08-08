use async_trait::async_trait;
use uuid::Uuid;

use crate::{
    RepoError,
    identity::{
        role::Role,
        session::{Session, SessionToken},
        user::User,
    },
};

/// Persistence operations for users.
#[async_trait]
pub trait UserRepository: Send + Sync + 'static {
    /// Insert a new user.
    async fn insert(&self, user: &User) -> Result<(), RepoError>;
    /// Find a user by its identifier.
    async fn find_by_id(&self, id: Uuid) -> Result<Option<User>, RepoError>;
    /// Find a user by username.
    async fn find_by_username(&self, username: &str) -> Result<Option<User>, RepoError>;
    /// Find a user by email address.
    async fn find_by_email(&self, email: &str) -> Result<Option<User>, RepoError>;
    /// List all users.
    async fn list(&self) -> Result<Vec<User>, RepoError>;
    /// Update a user's role.
    async fn update_role(&self, id: Uuid, role: Role) -> Result<(), RepoError>;
    /// Disable a user account.
    async fn disable(&self, id: Uuid) -> Result<(), RepoError>;
    /// Record the user's most recent login time.
    async fn update_last_login(&self, id: Uuid) -> Result<(), RepoError>;
    /// Update a user's password hash.
    async fn update_password(&self, id: Uuid, hash: &str) -> Result<(), RepoError>;
    /// Delete a user.
    async fn delete(&self, id: Uuid) -> Result<(), RepoError>;
    /// Count the number of users.
    async fn count(&self) -> Result<i64, RepoError>;
}

/// Persistence operations for sessions.
#[async_trait]
pub trait SessionRepository: Send + Sync + 'static {
    /// Insert a new session.
    async fn insert(&self, session: &Session) -> Result<(), RepoError>;
    /// Find a session by a matching token hash.
    async fn find_by_token_hash_match(
        &self,
        token: &SessionToken,
    ) -> Result<Option<Session>, RepoError>;
    /// Find a session by its identifier.
    async fn find_by_id(&self, id: Uuid) -> Result<Option<Session>, RepoError>;
    /// Refresh the session's activity timestamp.
    async fn touch(&self, id: Uuid) -> Result<(), RepoError>;
    /// Delete a session.
    async fn delete(&self, id: Uuid) -> Result<(), RepoError>;
    /// Delete all sessions belonging to a user.
    async fn delete_for_user(&self, user_id: Uuid) -> Result<(), RepoError>;
    /// Remove expired sessions and return the number removed.
    async fn purge_expired(&self) -> Result<u64, RepoError>;
}
