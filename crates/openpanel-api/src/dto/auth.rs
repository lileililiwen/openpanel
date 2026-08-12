use chrono::{DateTime, Utc};
use openpanel_domain::{Email, Role, User};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Request body for `POST /identity/login`.
#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    /// Username or email address identifying the account.
    pub username_or_email: String,
    /// Plaintext password (compared against the stored hash).
    pub password: String,
}

/// Response body for a successful `POST /identity/login`.
#[derive(Debug, Serialize)]
pub struct LoginResponse {
    /// Opaque bearer token clients should send in the `Authorization` header.
    pub token: String,
    /// Profile of the authenticated user.
    pub user: UserDto,
    /// Absolute UTC time at which `token` stops being valid.
    pub expires_at: DateTime<Utc>,
}

/// Response body when the password step succeeded but the user has a
/// second factor enrolled. The browser follows up with
/// `POST /identity/login/factor` carrying the challenge id, token, and
/// factor response.
#[derive(Debug, Serialize)]
pub struct LoginFactorRequired {
    /// Stable string the client echoes when comparing responses.
    pub status: &'static str,
    /// Stable challenge identifier.
    pub challenge_id: Uuid,
    /// When the challenge expires.
    pub expires_at: DateTime<Utc>,
}

/// Request body for `POST /identity/login/factor`. `kind` is
/// `"totp"` for a time-based OTP or `"recovery"` for a single-use
/// recovery code.
#[derive(Debug, Deserialize)]
pub struct LoginFactorRequest {
    /// Stable challenge identifier from the prior `factor_required` response.
    pub challenge_id: Uuid,
    /// Plaintext challenge token held by the browser.
    pub challenge_token: String,
    /// `"totp"` or `"recovery"`.
    pub kind: String,
    /// The TOTP code or recovery code.
    pub code: String,
}

/// Public-facing projection of a [`User`], safe to serialize over the wire.
#[derive(Debug, Serialize)]
pub struct UserDto {
    /// Stable user identifier.
    pub id: Uuid,
    /// Login name.
    pub username: String,
    /// Contact email address.
    pub email: String,
    /// Role-based authorization level.
    pub role: Role,
    /// Account creation timestamp (UTC).
    pub created_at: DateTime<Utc>,
    /// Set when the account was disabled, if applicable.
    pub disabled_at: Option<DateTime<Utc>>,
    /// Timestamp of the most recent successful login, if any.
    pub last_login_at: Option<DateTime<Utc>>,
}

impl UserDto {
    /// Projects a domain [`User`] into its wire DTO form.
    pub fn from_user(user: &User) -> Self {
        Self {
            id: user.id(),
            username: user.username().as_str().to_string(),
            email: user.email().as_str().to_string(),
            role: user.role(),
            created_at: user.created_at(),
            disabled_at: user.disabled_at(),
            last_login_at: user.last_login_at(),
        }
    }
}

impl UserDto {
    /// Parses and validates the embedded email string.
    pub fn email(&self) -> Result<Email, String> {
        Email::new(self.email.clone()).map_err(|e| e.to_string())
    }
}

/// Request body for `POST /identity/users` (owner-only).
#[derive(Debug, Deserialize)]
pub struct CreateUserRequest {
    /// Desired login name.
    pub username: String,
    /// Contact email for the new account.
    pub email: String,
    /// Initial plaintext password.
    pub password: String,
    /// Role to assign to the new user.
    pub role: Role,
}

/// Request body for `PATCH /identity/users/{id}` to change a user's role.
#[derive(Debug, Deserialize)]
pub struct ChangeRoleRequest {
    /// New role to assign.
    pub role: Role,
}

/// Request body for `POST /identity/users/{id}/password`.
#[derive(Debug, Deserialize)]
pub struct ChangePasswordRequest {
    /// Replacement plaintext password.
    pub new_password: String,
}
