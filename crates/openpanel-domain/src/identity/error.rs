use thiserror::Error;

/// Errors that can occur in the identity bounded context.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum IdentityError {
    /// The supplied credentials do not match.
    #[error("invalid credentials")]
    InvalidCredentials,
    /// The account has been disabled.
    #[error("account disabled")]
    AccountDisabled,
    /// The password is shorter than the minimum length.
    #[error("password too short (minimum 12 chars)")]
    PasswordTooShort,
    /// The username is already in use.
    #[error("username already taken")]
    UsernameTaken,
    /// The email address is already in use.
    #[error("email already taken")]
    EmailTaken,
    /// No user matches the given identifier.
    #[error("user not found")]
    UserNotFound,
    /// The session has expired.
    #[error("session expired")]
    SessionExpired,
    /// The caller lacks the required permission.
    #[error("permission denied")]
    Forbidden,
    /// The last owner cannot be demoted.
    #[error("cannot demote the last owner")]
    LastOwner,
    /// The session token is not valid.
    #[error("session token invalid")]
    InvalidToken,
    /// A persistence layer failure occurred.
    #[error("identity persistence error: {0}")]
    Persistence(String),
}
