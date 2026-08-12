//! Identity bounded context: users, sessions, roles, second-factor.

/// Error types for the identity context.
pub mod error;
/// Two-factor authentication: factor model, TOTP, recovery codes,
/// pending-login challenges.
pub mod factor;
/// Repository traits for users and sessions.
pub mod repository;
/// Roles and their permissions.
pub mod role;
/// Sessions, session tokens, and session building.
pub mod session;
/// The user aggregate.
pub mod user;

pub use error::IdentityError;
pub use factor::{
    Factor, FactorError, FactorKind, RecoveryCode, RecoveryCodeSet, TotpSecret, TwoFactorChallenge,
    constant_time_eq,
};
pub use repository::{SessionRepository, UserRepository};
pub use role::Role;
pub use session::{Session, SessionBuilder, SessionToken, SessionTokenError};
pub use user::User;
