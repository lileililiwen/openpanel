//! Domain layer: entities, value objects, aggregates, repository traits.
//! Zero I/O — no sqlx, no axum, no tokio. Implementation lives in
//! `openpanel-app`.

pub mod common;
pub mod identity;

pub use common::error::{DomainError, RepoError};
pub use common::{Email, Password, PasswordError, Username, UsernameError};
pub use identity::error::IdentityError;
pub use identity::repository::{SessionRepository, UserRepository};
pub use identity::role::Role;
pub use identity::session::{Session, SessionBuilder, SessionToken, SessionTokenError};
pub use identity::user::User;