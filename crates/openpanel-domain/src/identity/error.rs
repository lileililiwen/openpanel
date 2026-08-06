use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum IdentityError {
    #[error("invalid credentials")]
    InvalidCredentials,
    #[error("account disabled")]
    AccountDisabled,
    #[error("password too short (minimum 12 chars)")]
    PasswordTooShort,
    #[error("username already taken")]
    UsernameTaken,
    #[error("email already taken")]
    EmailTaken,
    #[error("user not found")]
    UserNotFound,
    #[error("session expired")]
    SessionExpired,
    #[error("permission denied")]
    Forbidden,
    #[error("cannot demote the last owner")]
    LastOwner,
    #[error("session token invalid")]
    InvalidToken,
    #[error("identity persistence error: {0}")]
    Persistence(String),
}