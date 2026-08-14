//! `Collaborator` and `SiteGrant` aggregates.

use chrono::{DateTime, Utc};
use crate::common::Email;
use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

use super::permission::PermissionSet;

/// Stable collaborator identifier (UUID v4).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CollaboratorId(Uuid);

impl CollaboratorId {
    /// Construct from a UUID.
    pub fn new(id: Uuid) -> Self {
        Self(id)
    }

    /// Construct a fresh, random collaborator id.
    pub fn generate() -> Self {
        Self(Uuid::new_v4())
    }

    /// Borrow the underlying UUID.
    pub fn as_uuid(&self) -> Uuid {
        self.0
    }
}

impl fmt::Display for CollaboratorId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for CollaboratorId {
    type Err = uuid::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

/// Collaborator lifecycle status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CollabStatus {
    /// Invite created, email not yet acknowledged.
    Invited,
    /// Collaborator accepted the invite and may act.
    Active,
    /// Collaborator was revoked by the account owner.
    Revoked,
}

impl CollabStatus {
    /// Stable string form.
    pub fn as_str(&self) -> &'static str {
        match self {
            CollabStatus::Invited => "invited",
            CollabStatus::Active => "active",
            CollabStatus::Revoked => "revoked",
        }
    }
}

/// A collaborator account scoped to a parent account.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Collaborator {
    /// Collaborator id.
    pub collaborator_id: CollaboratorId,
    /// Account id (the inviting account).
    pub account_id: Uuid,
    /// Collaborator email.
    pub email: Email,
    /// Current lifecycle status.
    pub status: CollabStatus,
    /// When the collaborator was invited.
    pub invited_at: DateTime<Utc>,
    /// When the collaborator accepted (None while still invited).
    pub accepted_at: Option<DateTime<Utc>>,
    /// When the collaborator was revoked (None while not revoked).
    pub revoked_at: Option<DateTime<Utc>>,
}

impl Collaborator {
    /// Construct a freshly-invited collaborator.
    pub fn new_invited(
        collaborator_id: CollaboratorId,
        account_id: Uuid,
        email: Email,
        now: DateTime<Utc>,
    ) -> Self {
        Self {
            collaborator_id,
            account_id,
            email,
            status: CollabStatus::Invited,
            invited_at: now,
            accepted_at: None,
            revoked_at: None,
        }
    }

    /// Mark the collaborator as accepted.
    pub fn accept(&mut self, now: DateTime<Utc>) {
        self.status = CollabStatus::Active;
        self.accepted_at = Some(now);
    }

    /// Mark the collaborator as revoked.
    pub fn revoke(&mut self, now: DateTime<Utc>) {
        self.status = CollabStatus::Revoked;
        self.revoked_at = Some(now);
    }

    /// Returns `true` if the collaborator is currently active.
    pub fn is_active(&self) -> bool {
        matches!(self.status, CollabStatus::Active)
    }
}

/// Site-scoped grant: a (collaborator, site, permissions) tuple.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SiteGrant {
    /// Collaborator receiving the grant.
    pub collaborator_id: CollaboratorId,
    /// Site the grant scopes the collaborator to.
    pub site_id: Uuid,
    /// Permission set (bit-flagged).
    pub permissions: PermissionSet,
    /// When the grant was created.
    pub granted_at: DateTime<Utc>,
}

impl SiteGrant {
    /// Construct a fresh grant.
    pub fn new(
        collaborator_id: CollaboratorId,
        site_id: Uuid,
        permissions: PermissionSet,
        now: DateTime<Utc>,
    ) -> Self {
        Self {
            collaborator_id,
            site_id,
            permissions,
            granted_at: now,
        }
    }

    /// Returns `true` if this grant allows `permission` on its site.
    pub fn allows(&self, permission: super::permission::Permission) -> bool {
        self.permissions.contains(permission)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collaborators::permission::Permission;

    #[test]
    fn collaborator_lifecycle() {
        let id = CollaboratorId::generate();
        let now = Utc::now();
        let mut c = Collaborator::new_invited(
            id.clone(),
            Uuid::new_v4(),
            Email::new("dev@example.com").unwrap(),
            now,
        );
        assert!(!c.is_active());
        c.accept(now);
        assert!(c.is_active());
        c.revoke(now);
        assert!(!c.is_active());
    }

    #[test]
    fn site_grant_allows() {
        let grant = SiteGrant::new(
            CollaboratorId::generate(),
            Uuid::new_v4(),
            PermissionSet::from_iter([Permission::File, Permission::Database]),
            Utc::now(),
        );
        assert!(grant.allows(Permission::File));
        assert!(!grant.allows(Permission::Mail));
    }
}