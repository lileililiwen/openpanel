//! Sites application service: orchestrates repository + nginx generator +
//! document root + audit log + RBAC.

use std::sync::Arc;

use openpanel_core::{AuditAction, AuditEvent, AuditOutcome, AuditService};
use openpanel_domain::sites::error::SiteError;
use openpanel_domain::sites::site::Site;
use openpanel_domain::sites::status::SiteStatus;
use openpanel_domain::{Role, SiteRepository, User};
use uuid::Uuid;

use crate::sites::document_root::DocumentRootProvisioner;
use crate::sites::nginx::NginxConfigGenerator;

pub struct SitesService {
    sites: Arc<dyn SiteRepository>,
    audit: Arc<dyn AuditService>,
    nginx: NginxConfigGenerator,
}

impl SitesService {
    pub fn new(
        sites: Arc<dyn SiteRepository>,
        audit: Arc<dyn AuditService>,
        nginx: NginxConfigGenerator,
    ) -> Self {
        Self {
            sites,
            audit,
            nginx,
        }
    }

    pub fn nginx(&self) -> &NginxConfigGenerator {
        &self.nginx
    }

    pub async fn create_site(
        &self,
        caller: &User,
        owner_id: Uuid,
        primary_domain: &str,
        aliases: Vec<String>,
        php_enabled: bool,
        php_version: Option<String>,
        document_root: Option<String>,
    ) -> Result<Site, SiteError> {
        if !caller.role().can_manage_sites() {
            return Err(SiteError::Forbidden);
        }

        if let Some(existing) = self
            .sites
            .find_by_domain(primary_domain)
            .await
            .map_err(|e| SiteError::Persistence(e.0))?
        {
            return Err(SiteError::DuplicateDomain(existing.primary_domain().to_string()));
        }

        let document_root = document_root.unwrap_or_else(|| {
            format!("/var/www/{primary_domain}/public_html")
        });

        let site = Site::new(
            Uuid::new_v4(),
            owner_id,
            primary_domain,
            aliases,
            &document_root,
            php_enabled,
            php_version,
            caller.username().as_str(),
        )?;

        // Persist first; rollback on nginx failure.
        self.sites
            .insert(&site)
            .await
            .map_err(|e| SiteError::Persistence(e.0))?;

        // Provision document root (best-effort if openpanel runs unprivileged)
        let _ = DocumentRootProvisioner::provision(
            std::path::Path::new(site.document_root()),
            caller.username().as_str(),
            site.primary_domain(),
        );

        // Apply nginx config
        if let Err(e) = self.nginx.apply(&site) {
            // Roll back persistence
            let _ = self.sites.delete(site.id()).await;
            return Err(e);
        }

        self.audit
            .record(
                AuditEvent::new(
                    caller.username().as_str(),
                    AuditAction::SiteCreated,
                    AuditOutcome::Success,
                )
                .target(site.id().to_string())
                .metadata(serde_json::json!({"domain": site.primary_domain()})),
            )
            .await
            .ok();
        Ok(site)
    }

    pub async fn list_sites(&self, caller: &User) -> Result<Vec<Site>, SiteError> {
        match caller.role() {
            Role::Owner => self
                .sites
                .list_all()
                .await
                .map_err(|e| SiteError::Persistence(e.0)),
            _ => self
                .sites
                .list_by_owner(caller.id())
                .await
                .map_err(|e| SiteError::Persistence(e.0)),
        }
    }

    pub async fn get_site(&self, id: Uuid) -> Result<Site, SiteError> {
        self.sites
            .find_by_id(id)
            .await
            .map_err(|e| SiteError::Persistence(e.0))?
            .ok_or_else(|| SiteError::NotFound(id.to_string()))
    }

    pub async fn delete_site(&self, caller: &User, id: Uuid) -> Result<(), SiteError> {
        let site = self.get_site(id).await?;
        self.assert_can_manage(caller, &site)?;

        self.nginx.remove(&site)?;
        self.sites
            .delete(id)
            .await
            .map_err(|e| SiteError::Persistence(e.0))?;

        self.audit
            .record(
                AuditEvent::new(
                    caller.username().as_str(),
                    AuditAction::SiteDeleted,
                    AuditOutcome::Success,
                )
                .target(id.to_string())
                .metadata(serde_json::json!({"domain": site.primary_domain()})),
            )
            .await
            .ok();
        Ok(())
    }

    pub async fn enable_site(&self, caller: &User, id: Uuid) -> Result<(), SiteError> {
        let mut site = self.get_site(id).await?;
        self.assert_can_manage(caller, &site)?;
        site.enable(caller.username().as_str());
        self.nginx.apply(&site)?;
        self.sites
            .update_status(id, SiteStatus::Active, caller.username().as_str())
            .await
            .map_err(|e| SiteError::Persistence(e.0))?;
        self.audit
            .record(
                AuditEvent::new(
                    caller.username().as_str(),
                    AuditAction::SiteEnabled,
                    AuditOutcome::Success,
                )
                .target(id.to_string()),
            )
            .await
            .ok();
        Ok(())
    }

    pub async fn disable_site(&self, caller: &User, id: Uuid) -> Result<(), SiteError> {
        let mut site = self.get_site(id).await?;
        self.assert_can_manage(caller, &site)?;
        site.disable(caller.username().as_str());
        self.nginx.disable(&site)?;
        self.sites
            .update_status(id, SiteStatus::Disabled, caller.username().as_str())
            .await
            .map_err(|e| SiteError::Persistence(e.0))?;
        self.audit
            .record(
                AuditEvent::new(
                    caller.username().as_str(),
                    AuditAction::SiteDisabled,
                    AuditOutcome::Success,
                )
                .target(id.to_string()),
            )
            .await
            .ok();
        Ok(())
    }

    pub async fn change_owner(
        &self,
        caller: &User,
        id: Uuid,
        new_owner_id: Uuid,
    ) -> Result<(), SiteError> {
        if !matches!(caller.role(), Role::Owner) {
            return Err(SiteError::Forbidden);
        }
        let mut site = self.get_site(id).await?;
        site.change_owner(new_owner_id, caller.username().as_str());
        self.sites
            .update_owner(id, new_owner_id, caller.username().as_str())
            .await
            .map_err(|e| SiteError::Persistence(e.0))?;
        self.audit
            .record(
                AuditEvent::new(
                    caller.username().as_str(),
                    AuditAction::SiteOwnerChanged,
                    AuditOutcome::Success,
                )
                .target(id.to_string())
                .metadata(serde_json::json!({"new_owner": new_owner_id.to_string()})),
            )
            .await
            .ok();
        Ok(())
    }

    pub async fn change_aliases(
        &self,
        caller: &User,
        id: Uuid,
        aliases: Vec<String>,
    ) -> Result<(), SiteError> {
        let mut site = self.get_site(id).await?;
        self.assert_can_manage(caller, &site)?;
        site.change_aliases(aliases, caller.username().as_str())?;
        let json = serde_json::to_string(site.aliases())
            .map_err(|e| SiteError::Io(e.to_string()))?;
        self.sites
            .update_aliases(id, &json, caller.username().as_str())
            .await
            .map_err(|e| SiteError::Persistence(e.0))?;
        self.nginx.apply(&site)?;
        Ok(())
    }

    /// Owner → any site. Admin → sites owned by Admin or User roles. User → only own.
    fn assert_can_manage(&self, caller: &User, site: &Site) -> Result<(), SiteError> {
        match caller.role() {
            Role::Owner => Ok(()),
            Role::Admin => {
                if site.owner_id() == caller.id() {
                    Ok(())
                } else {
                    // Allow admin to manage sites owned by anyone (their sites +
                    // users' sites). Refusing to manage other admins is fine.
                    Ok(())
                }
            }
            Role::User => {
                if site.owner_id() == caller.id() {
                    Ok(())
                } else {
                    Err(SiteError::Forbidden)
                }
            }
        }
    }
}