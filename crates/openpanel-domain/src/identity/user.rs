use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    common::{Email, Password, Username},
    identity::{error::IdentityError, role::Role},
};

/// A registered user of the system.
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
    /// Create a new user with the given identity and role.
    pub fn new(id: Uuid, username: Username, email: Email, password: Password, role: Role) -> Self {
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

    /// Disable the user account.
    pub fn disable(&mut self) {
        self.disabled_at = Some(Utc::now());
    }

    /// Record the current time as the user's last login.
    pub fn record_login(&mut self) {
        self.last_login_at = Some(Utc::now());
    }

    /// Change the user's role, refusing to demote the last owner.
    pub fn change_role(&mut self, role: Role) -> Result<(), IdentityError> {
        if matches!(self.role, Role::Owner) && !matches!(role, Role::Owner) {
            // Refuse to demote the last owner. Single-user MVP — refuse always
            // until a multi-owner check lands.
            return Err(IdentityError::LastOwner);
        }
        self.role = role;
        Ok(())
    }

    /// Change the user's password, hashing the new plaintext.
    pub fn change_password(&mut self, new_plaintext: &str) -> Result<(), IdentityError> {
        let hash = Password::hash(new_plaintext).map_err(|_| IdentityError::PasswordTooShort)?;
        self.password = hash;
        Ok(())
    }

    /// Return the user's identifier.
    pub fn id(&self) -> Uuid {
        self.id
    }

    /// Return the user's username.
    pub fn username(&self) -> &Username {
        &self.username
    }

    /// Return the user's email address.
    pub fn email(&self) -> &Email {
        &self.email
    }

    /// Return the user's password value.
    pub fn password(&self) -> &Password {
        &self.password
    }

    /// Return the user's role.
    pub fn role(&self) -> Role {
        self.role
    }

    /// Return when the user was created.
    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    /// Return when the user was disabled, if ever.
    pub fn disabled_at(&self) -> Option<DateTime<Utc>> {
        self.disabled_at
    }

    /// Return when the user last logged in, if ever.
    pub fn last_login_at(&self) -> Option<DateTime<Utc>> {
        self.last_login_at
    }

    /// Whether the user account is disabled.
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
