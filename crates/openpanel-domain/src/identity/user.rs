use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    common::{Email, Password, Username},
    hosting::HostingPlanId,
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
    parent_account_id: Option<Uuid>,
    hosting_plan_id: Option<HostingPlanId>,
    created_at: DateTime<Utc>,
    disabled_at: Option<DateTime<Utc>>,
    last_login_at: Option<DateTime<Utc>>,
}

impl User {
    /// Create a new user with the given identity and role. The
    /// hierarchy and hosting-plan fields default to `None`; use
    /// [`User::new_with_parents`] when tree placement or plan
    /// attachment is required at creation time.
    pub fn new(id: Uuid, username: Username, email: Email, password: Password, role: Role) -> Self {
        // `new_with_parents` only rejects when `parent_account_id ==
        // Some(id)`. With both fields `None`, the validation cannot
        // fail. Building the aggregate inline avoids relying on a
        // panic that would otherwise trigger `clippy::expect_used`
        // and is clearer at the construction site.
        Self {
            id,
            username,
            email,
            password,
            role,
            parent_account_id: None,
            hosting_plan_id: None,
            created_at: Utc::now(),
            disabled_at: None,
            last_login_at: None,
        }
    }

    /// Create a new user with hierarchy and hosting-plan references.
    ///
    /// Self-parenting — `parent_account_id == Some(id)` — is rejected
    /// with [`IdentityError::ParentAccountCycle`]. The follow-on
    /// `add-account-hierarchy` change extends the cycle check to
    /// transitive cycles (e.g. A → B → A); this constructor only
    /// guards the direct self-loop as the largest invariant that can
    /// be enforced without a database lookup.
    pub fn new_with_parents(
        id: Uuid,
        username: Username,
        email: Email,
        password: Password,
        role: Role,
        parent_account_id: Option<Uuid>,
        hosting_plan_id: Option<HostingPlanId>,
    ) -> Result<Self, IdentityError> {
        if let Some(parent) = parent_account_id
            && parent == id
        {
            return Err(IdentityError::ParentAccountCycle);
        }
        Ok(Self {
            id,
            username,
            email,
            password,
            role,
            parent_account_id,
            hosting_plan_id,
            created_at: Utc::now(),
            disabled_at: None,
            last_login_at: None,
        })
    }

    /// Disable the user account.
    pub fn disable(&mut self) {
        self.disabled_at = Some(Utc::now());
    }

    /// Re-enable a previously disabled user account.
    pub fn enable(&mut self) {
        self.disabled_at = None;
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

    /// Attach the user to a parent account. Refuses self-parenting.
    pub fn set_parent_account_id(&mut self, parent: Option<Uuid>) -> Result<(), IdentityError> {
        if let Some(parent_id) = parent
            && parent_id == self.id
        {
            return Err(IdentityError::ParentAccountCycle);
        }
        self.parent_account_id = parent;
        Ok(())
    }

    /// Attach the user to a hosting plan.
    pub fn set_hosting_plan_id(&mut self, plan: Option<HostingPlanId>) {
        self.hosting_plan_id = plan;
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

    /// Return the parent account id, if any.
    pub fn parent_account_id(&self) -> Option<Uuid> {
        self.parent_account_id
    }

    /// Return the hosting plan id, if any.
    pub fn hosting_plan_id(&self) -> Option<HostingPlanId> {
        self.hosting_plan_id
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
        parent_account_id: Option<Uuid>,
        hosting_plan_id: Option<HostingPlanId>,
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
            parent_account_id,
            hosting_plan_id,
            created_at,
            disabled_at,
            last_login_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user_fields() -> (Uuid, Username, Email, Password, Role) {
        Password::set_test_costs(8, 1);
        let id = Uuid::new_v4();
        let username = Username::new("alice".to_string()).expect("username");
        let email = Email::new("alice@example.com".to_string()).expect("email");
        let password = Password::hash("correct horse battery staple").expect("password");
        let role = Role::Owner;
        (id, username, email, password, role)
    }

    #[test]
    fn new_without_parents_has_none_for_both_fields() {
        let (id, username, email, password, role) = user_fields();
        let user = User::new(id, username, email, password, role);
        assert_eq!(user.parent_account_id(), None);
        assert_eq!(user.hosting_plan_id(), None);
    }

    #[test]
    fn new_with_parents_assigns_both_fields() {
        let (id, username, email, password, role) = user_fields();
        let parent = Uuid::new_v4();
        let plan = HostingPlanId::new();
        let user = User::new_with_parents(
            id,
            username,
            email,
            password,
            role,
            Some(parent),
            Some(plan),
        )
        .expect("non-self parent is accepted");
        assert_eq!(user.parent_account_id(), Some(parent));
        assert_eq!(user.hosting_plan_id(), Some(plan));
    }

    #[test]
    fn new_with_self_parent_is_rejected() {
        let (id, username, email, password, role) = user_fields();
        let err = User::new_with_parents(id, username, email, password, role, Some(id), None)
            .expect_err("self-parent must be rejected");
        assert_eq!(err, IdentityError::ParentAccountCycle);
    }

    #[test]
    fn set_parent_account_id_rejects_self_loop() {
        let (id, username, email, password, role) = user_fields();
        let mut user = User::new(id, username, email, password, role);
        let err = user
            .set_parent_account_id(Some(id))
            .expect_err("setting self as parent must fail");
        assert_eq!(err, IdentityError::ParentAccountCycle);
    }

    #[test]
    fn set_hosting_plan_id_round_trips() {
        let (id, username, email, password, role) = user_fields();
        let mut user = User::new(id, username, email, password, role);
        let plan = HostingPlanId::new();
        user.set_hosting_plan_id(Some(plan));
        assert_eq!(user.hosting_plan_id(), Some(plan));
        user.set_hosting_plan_id(None);
        assert_eq!(user.hosting_plan_id(), None);
    }

    #[test]
    fn restore_preserves_hierarchy_and_plan_fields() {
        let id = Uuid::new_v4();
        let username = Username::new("bob".to_string()).expect("username");
        let email = Email::new("bob@example.com".to_string()).expect("email");
        let parent = Uuid::new_v4();
        let plan = HostingPlanId::new();
        let user = User::restore(
            id,
            username,
            email,
            "argon2id$...".to_string(),
            Role::Admin,
            Some(parent),
            Some(plan),
            Utc::now(),
            None,
            None,
        );
        assert_eq!(user.parent_account_id(), Some(parent));
        assert_eq!(user.hosting_plan_id(), Some(plan));
    }
}
