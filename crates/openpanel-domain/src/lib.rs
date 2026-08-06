//! Domain layer: entities, value objects, aggregates, repository traits.
//! Zero I/O — no sqlx, no axum, no tokio. Implementation lives in
//! `openpanel-app`.

pub mod common;
pub mod databases;
pub mod identity;
pub mod sites;

pub use common::error::{DomainError, RepoError};
pub use common::{Email, Password, PasswordError, Username, UsernameError};
pub use databases::database::Database;
pub use databases::engine::DatabaseEngine;
pub use databases::error::DatabaseError;
pub use databases::repository::DatabaseRepository;
pub use databases::status::DatabaseStatus;
pub use identity::error::IdentityError;
pub use identity::repository::{SessionRepository, UserRepository};
pub use identity::role::Role;
pub use identity::session::{Session, SessionBuilder, SessionToken, SessionTokenError};
pub use identity::user::User;
pub use sites::error::SiteError;
pub use sites::repository::SiteRepository;
pub use sites::site::Site;
pub use sites::status::SiteStatus;