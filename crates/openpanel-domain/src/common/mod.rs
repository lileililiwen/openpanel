//! Shared value objects: email, username, password.

pub mod email;
pub mod error;
pub mod password;
pub mod username;

pub use email::Email;
pub use error::DomainError;
pub use password::{Password, PasswordError};
pub use username::{Username, UsernameError};
