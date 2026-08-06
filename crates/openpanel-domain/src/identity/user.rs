use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::common::{Email, Password, Username};
use crate::identity::error::IdentityError;
use crate::identity::role::Role;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    id: Uuid,
    username: Username,
    email: Email,
    password: Password,
    role: Role,
    created_at: DateTime<Utc>,
    disabled_at: Option<DateTime<Utc>>,
    last_login_at: Option<DateTime<Utc>>,
}

impl User {
    pub fn new(
        id: Uuid,
        username: Username,
        email: Email,
        password: Password,
        role: Role,
    ) -> Self {
        Self {
            id,
            username,
            email,
            password,
            role,
            created_at: Utc::now(),
            disabled_at: None,
            last_login_at: None,
        }
    }

    pub fn disable(&mut self) {
        self.disabled_at = Some(Utc::now());
    }

    pub fn record_login(&mut self) {
        self.last_login_at = Some(Utc::now());
    }

    pub fn change_role(&mut self, role: Role) -> Result<(), IdentityError> {
        if matches!(self.role, Role::Owner) && !matches!(role, Role::Owner) {
            // Refuse to demote the last owner. Single-user MVP — refuse always
            // until a multi-owner check lands.
            return Err(IdentityError::LastOwner);
        }
        self.role = role;
        Ok(())
    }

    pub fn change_password(&mut self, new_plaintext: &str) -> Result<(), IdentityError> {
        let hash = Password::hash(new_plaintext).map_err(|_| IdentityError::PasswordTooShort)?;
        self.password = hash;
        Ok(())
    }

    pub fn id(&self) -> Uuid {
        self.id
    }
    pub fn username(&self) -> &Username {
        &self.username
    }
    pub fn email(&self) -> &Email {
        &self.email
    }
    pub fn password(&self) -> &Password {
        &self.password
    }
    pub fn role(&self) -> Role {
        self.role
    }
    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    pub fn disabled_at(&self) -> Option<DateTime<Utc>> {
        self.disabled_at
    }
    pub fn last_login_at(&self) -> Option<DateTime<Utc>> {
        self.last_login_at
    }
    pub fn is_disabled(&self) -> bool {
        self.disabled_at.is_some()
    }

    /// Restore from persistence. Used by the SQLite repository adapter.
    #[allow(clippy::too_many_arguments)]
    pub fn restore(
        id: Uuid,
        username: Username,
        email: Email,
        password_hash: String,
        role: Role,
        created_at: DateTime<Utc>,
        disabled_at: Option<DateTime<Utc>>,
        last_login_at: Option<DateTime<Utc>>,
    ) -> Self {
        Self {
            id,
            username,
            email,
            password: Password::from_hash(password_hash),
            role,
            created_at,
            disabled_at,
            last_login_at,
        }
    }
}