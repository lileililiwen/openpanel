//! Identity bounded context: users, sessions, roles, second-factor.

/// Error types for the identity context.
pub mod error;
/// Two-factor authentication: factor model, TOTP, recovery codes,
/// pending-login challenges.
pub mod factor;
/// Remember-device cookie payload (stateless HMAC-signed token).
pub mod remember;
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
    Factor, FactorError, FactorKind, RecoveryCode, RecoveryCodeSet, TotpEnrollmentChallenge,
    TotpSecret, TwoFactorChallenge, UserVerificationPolicy, WebAuthnChallenge, WebAuthnCredential,
    constant_time_eq,
};
pub use remember::{
    REMEMBER_DEVICE_COOKIE, REMEMBER_DEVICE_DEFAULT_LIFETIME, REMEMBER_DEVICE_VERSION,
    RememberedDevicePayload, hash_user_agent, ip_prefix,
};
pub use repository::{SessionRepository, UserRepository};
pub use role::Role;
pub use session::{Session, SessionBuilder, SessionToken, SessionTokenError};
pub use user::User;
