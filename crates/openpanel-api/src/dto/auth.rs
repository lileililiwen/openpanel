use chrono::{DateTime, Utc};
use openpanel_domain::{Email, Role, User};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub username_or_email: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub token: String,
    pub user: UserDto,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct UserDto {
    pub id: Uuid,
    pub username: String,
    pub email: String,
    pub role: Role,
    pub created_at: DateTime<Utc>,
    pub disabled_at: Option<DateTime<Utc>>,
    pub last_login_at: Option<DateTime<Utc>>,
}

impl UserDto {
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
    pub fn email(&self) -> Result<Email, String> {
        Email::new(self.email.clone()).map_err(|e| e.to_string())
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateUserRequest {
    pub username: String,
    pub email: String,
    pub password: String,
    pub role: Role,
}

#[derive(Debug, Deserialize)]
pub struct ChangeRoleRequest {
    pub role: Role,
}

#[derive(Debug, Deserialize)]
pub struct ChangePasswordRequest {
    pub new_password: String,
}