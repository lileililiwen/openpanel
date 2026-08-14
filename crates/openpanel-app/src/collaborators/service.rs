//! Collaborator service: invite, resolve, revoke.

use std::sync::Arc;

use chrono::Utc;
use openpanel_core::audit::{AuditAction, AuditEvent, AuditOutcome, AuditService};
use openpanel_domain::common::error::RepoError;
use openpanel_domain::{
    Collaborator, CollaboratorError, CollaboratorId, CollaboratorRepository, Email, Permission,
    PermissionSet, SiteGrant, SiteGrantRepository,
};
use uuid::Uuid;

/// Request to invite a new collaborator.
#[derive(Debug, Clone)]
pub struct InviteRequest {
    /// Collaborator email.
    pub email: String,
    /// Site the collaborator will be scoped to.
    pub site_id: Uuid,
    /// Permission set the collaborator will receive.
    pub permissions: PermissionSet,
}

/// Request to update a collaborator's permissions on a site.
#[derive(Debug, Clone)]
pub struct UpdateRequest {
    /// New permission set.
    pub permissions: PermissionSet,
}

/// Errors raised by the collaborator service.
#[derive(Debug, thiserror::Error)]
pub enum InviteCollaboratorError {
    /// Domain / validation failure.
    #[error(transparent)]
    Domain(#[from] CollaboratorError),
    /// Persistence failure.
    #[error("persistence error: {0}")]
    Persistence(String),
}

impl From<RepoError> for InviteCollaboratorError {
    fn from(e: RepoError) -> Self {
        Self::Persistence(e.0)
    }
}

/// Resolves a collaborator's effective permission set for a site.
pub struct GrantResolver {
    grants: Arc<dyn SiteGrantRepository>,
}

impl GrantResolver {
    /// Construct a new grant resolver.
    pub fn new(grants: Arc<dyn SiteGrantRepository>) -> Self {
        Self { grants }
    }

    /// Resolve the union of permissions held by `collaborator_id`
    /// for `site_id`. The returned set NEVER includes the account
    /// role; it is exactly the union of `SiteGrant` permissions.
    pub async fn resolve(
        &self,
        collaborator_id: &CollaboratorId,
        site_id: Uuid,
    ) -> Result<PermissionSet, InviteCollaboratorError> {
        let grant = self
            .grants
            .find(collaborator_id, site_id)
            .await?
            .ok_or_else(|| {
                InviteCollaboratorError::Domain(CollaboratorError::GrantMissing(
                    collaborator_id.to_string(),
                    site_id.to_string(),
                ))
            })?;
        Ok(grant.permissions)
    }

    /// Returns `true` if the collaborator has `permission` on the site.
    pub async fn allows(
        &self,
        collaborator_id: &CollaboratorId,
        site_id: Uuid,
        permission: Permission,
    ) -> Result<bool, InviteCollaboratorError> {
        let set = self.resolve(collaborator_id, site_id).await?;
        Ok(set.contains(permission))
    }
}

/// Per-site collaborator service.
pub struct CollaboratorService {
    collaborators: Arc<dyn CollaboratorRepository>,
    grants: Arc<dyn SiteGrantRepository>,
    audit: Arc<dyn AuditService>,
}

impl CollaboratorService {
    /// Construct a new service.
    pub fn new(
        collaborators: Arc<dyn CollaboratorRepository>,
        grants: Arc<dyn SiteGrantRepository>,
        audit: Arc<dyn AuditService>,
    ) -> Self {
        Self {
            collaborators,
            grants,
            audit,
        }
    }

    /// Invite a collaborator with the given email and permissions.
    /// Creates both a `Collaborator` (status `Invited`) and a
    /// `SiteGrant` for `site_id`. Audit `CollaboratorInvited`.
    pub async fn invite(
        &self,
        account_id: Uuid,
        request: InviteRequest,
        actor: &str,
    ) -> Result<Collaborator, InviteCollaboratorError> {
        let email = Email::new(request.email.clone()).map_err(|_| {
            InviteCollaboratorError::Domain(CollaboratorError::ScopeNotAllowed(
                "invalid email".into(),
            ))
        })?;
        if request.permissions.is_empty() {
            return Err(InviteCollaboratorError::Domain(
                CollaboratorError::ScopeNotAllowed("empty permission set".into()),
            ));
        }
        let now = Utc::now();
        let collaborator_id = CollaboratorId::generate();
        let collaborator = Collaborator::new_invited(
            collaborator_id.clone(),
            account_id,
            email,
            now,
        );
        self.collaborators.insert(&collaborator).await?;
        let grant = SiteGrant::new(
            collaborator_id.clone(),
            request.site_id,
            request.permissions,
            now,
        );
        self.grants.insert(&grant).await?;
        let scopes: Vec<&'static str> = request
            .permissions
            .iter()
            .map(|p| p.as_str())
            .collect();
        let event = AuditEvent::new(
            actor,
            AuditAction::CollaboratorInvited,
            AuditOutcome::Success,
        )
        .metadata(serde_json::json!({
            "site": request.site_id,
            "scopes": scopes,
        }));
        self.audit.record(event).await.ok();
        Ok(collaborator)
    }

    /// Update the permissions on an existing site grant. Audit
    /// `CollaboratorUpdated`.
    pub async fn update_permissions(
        &self,
        collaborator_id: &CollaboratorId,
        site_id: Uuid,
        request: UpdateRequest,
        actor: &str,
    ) -> Result<SiteGrant, InviteCollaboratorError> {
        self.grants
            .update_permissions(collaborator_id, site_id, request.permissions)
            .await?;
        let grant = self
            .grants
            .find(collaborator_id, site_id)
            .await?
            .ok_or_else(|| {
                InviteCollaboratorError::Domain(CollaboratorError::GrantMissing(
                    collaborator_id.to_string(),
                    site_id.to_string(),
                ))
            })?;
        let scopes: Vec<&'static str> = grant.permissions.iter().map(|p| p.as_str()).collect();
        let event = AuditEvent::new(
            actor,
            AuditAction::CollaboratorUpdated,
            AuditOutcome::Success,
        )
        .metadata(serde_json::json!({
            "site": site_id,
            "scopes": scopes,
        }));
        self.audit.record(event).await.ok();
        Ok(grant)
    }

    /// Revoke a collaborator from a single site. Removes the grant
    /// (and only the grant). The collaborator account itself is left
    /// untouched — the user can still collaborate on other sites if
    /// they hold grants there. Audit `CollaboratorRevoked`.
    pub async fn revoke(
        &self,
        collaborator_id: &CollaboratorId,
        site_id: Uuid,
        actor: &str,
    ) -> Result<(), InviteCollaboratorError> {
        self.grants.delete(collaborator_id, site_id).await?;
        let now = Utc::now();
        if let Some(mut c) = self.collaborators.find(collaborator_id).await? {
            // Only flip the lifecycle to Revoked when there are no
            // remaining grants; otherwise the collaborator remains
            // Active and just loses this site.
            let remaining = self
                .grants
                .list_for_collaborator(collaborator_id)
                .await?
                .len();
            if remaining == 0 {
                c.revoke(now);
                self.collaborators.update_status(collaborator_id, &c).await?;
            }
        }
        let event = AuditEvent::new(
            actor,
            AuditAction::CollaboratorRevoked,
            AuditOutcome::Success,
        )
        .metadata(serde_json::json!({"site": site_id}));
        self.audit.record(event).await.ok();
        Ok(())
    }

    /// Look up a collaborator by id.
    pub async fn find(
        &self,
        id: &CollaboratorId,
    ) -> Result<Option<Collaborator>, InviteCollaboratorError> {
        Ok(self.collaborators.find(id).await?)
    }

    /// List all collaborators for an account.
    pub async fn list_for_account(
        &self,
        account_id: Uuid,
    ) -> Result<Vec<Collaborator>, InviteCollaboratorError> {
        Ok(self.collaborators.list_for_account(account_id).await?)
    }

    /// List all grants on a single site.
    pub async fn grants_for_site(
        &self,
        site_id: Uuid,
    ) -> Result<Vec<SiteGrant>, InviteCollaboratorError> {
        Ok(self.grants.list_for_site(site_id).await?)
    }
}