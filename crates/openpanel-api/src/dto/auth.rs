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

/// Public-facing projection of an enrolled factor.
#[derive(Debug, Serialize)]
pub struct FactorDto {
    /// Stable factor identifier.
    pub id: Uuid,
    /// Owner user id.
    pub user_id: Uuid,
    /// Factor kind (`"totp"` today; `"webauthn"` in Phase B).
    pub kind: String,
    /// When the factor was enrolled (UTC).
    pub created_at: DateTime<Utc>,
    /// When the factor was verified (UTC).
    pub verified_at: DateTime<Utc>,
    /// When the factor was last used successfully, if applicable.
    pub last_used_at: Option<DateTime<Utc>>,
    /// When the factor was revoked, if applicable.
    pub revoked_at: Option<DateTime<Utc>>,
}

impl FactorDto {
    /// Project a domain [`Factor`](openpanel_domain::identity::Factor) into its wire DTO form.
    pub fn from_factor(factor: &openpanel_domain::identity::Factor) -> Self {
        Self {
            id: factor.id(),
            user_id: factor.user_id(),
            kind: factor.kind().to_string(),
            created_at: factor.created_at(),
            verified_at: factor.verified_at(),
            last_used_at: factor.last_used_at(),
            revoked_at: factor.revoked_at(),
        }
    }
}

/// Response body for `GET /identity/factors`.
#[derive(Debug, Serialize)]
pub struct FactorListResponse {
    /// All of the user's factors (active and revoked).
    pub factors: Vec<FactorDto>,
    /// How many recovery codes remain.
    pub recovery_codes_remaining: u8,
}

/// Response body for `POST /identity/factors/totp/enroll`. The
/// `secret_base32`, `provisioning_uri`, and `recovery_codes` are shown
/// exactly once; the panel never returns them again.
#[derive(Debug, Serialize)]
pub struct EnrollTotpResponse {
    /// The freshly created factor.
    pub factor: FactorDto,
    /// Base32-encoded TOTP secret for manual entry into authenticator apps.
    pub secret_base32: String,
    /// otpauth:// URI for QR provisioning.
    pub provisioning_uri: String,
    /// Freshly generated recovery codes (single-use, shown once).
    pub recovery_codes: Vec<String>,
}

/// Response body for `POST /identity/factors/recovery/regenerate`.
#[derive(Debug, Serialize)]
pub struct RegenerateRecoveryResponse {
    /// Freshly generated recovery codes (shown once).
    pub recovery_codes: Vec<String>,
}
