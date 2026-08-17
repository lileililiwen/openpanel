//! `CollaboratorRepository` and `SiteGrantRepository` traits.

use async_trait::async_trait;
use uuid::Uuid;

use super::{
    grant::{Collaborator, CollaboratorId, SiteGrant},
    permission::PermissionSet,
};
use crate::common::error::RepoError;

/// Repository for collaborator accounts.
#[async_trait]
pub trait CollaboratorRepository: Send + Sync {
    /// Insert a freshly-invited collaborator.
    async fn insert(&self, collaborator: &Collaborator) -> Result<(), RepoError>;

    /// Look up a collaborator by id.
    async fn find(&self, id: &CollaboratorId) -> Result<Option<Collaborator>, RepoError>;

    /// Update collaborator lifecycle status (accept / revoke).
    async fn update_status(
        &self,
        id: &CollaboratorId,
        collaborator: &Collaborator,
    ) -> Result<(), RepoError>;

    /// List collaborators for an account.
    async fn list_for_account(&self, account_id: Uuid) -> Result<Vec<Collaborator>, RepoError>;
}

/// Repository for site-scoped grants.
#[async_trait]
pub trait SiteGrantRepository: Send + Sync {
    /// Insert a fresh grant.
    async fn insert(&self, grant: &SiteGrant) -> Result<(), RepoError>;

    /// Remove a grant for a (collaborator, site) pair.
    async fn delete(
        &self,
        collaborator_id: &CollaboratorId,
        site_id: Uuid,
    ) -> Result<(), RepoError>;

    /// Update a grant's permission set.
    async fn update_permissions(
        &self,
        collaborator_id: &CollaboratorId,
        site_id: Uuid,
        permissions: PermissionSet,
    ) -> Result<(), RepoError>;

    /// Look up grants for a (collaborator, site) pair.
    async fn find(
        &self,
        collaborator_id: &CollaboratorId,
        site_id: Uuid,
    ) -> Result<Option<SiteGrant>, RepoError>;

    /// List all grants held by a collaborator across all sites.
    async fn list_for_collaborator(
        &self,
        collaborator_id: &CollaboratorId,
    ) -> Result<Vec<SiteGrant>, RepoError>;

    /// List all grants on a single site.
    async fn list_for_site(&self, site_id: Uuid) -> Result<Vec<SiteGrant>, RepoError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collaborators::permission::Permission;

    #[test]
    fn permission_set_persists_as_u8() {
        let s = PermissionSet::from_iter([Permission::File, Permission::Cron]);
        let json = serde_json::to_string(&s).unwrap();
        assert_eq!(json, "9");
        let back: PermissionSet = serde_json::from_str(&json).unwrap();
        assert_eq!(back, s);
    }
}
