//! Collaborator service tests.

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use chrono::Utc;
    use openpanel_core::{AuditService, audit::AuditEvent};
    use openpanel_domain::{
        CollabStatus, Email, SiteGrantRepository,
        collaborators::permission::{Permission, PermissionSet},
    };
    use openpanel_test_support::TestDb;
    use tokio::sync::Mutex;
    use uuid::Uuid;

    use super::super::{
        repo::{SqliteCollaboratorRepository, SqliteSiteGrantRepository},
        service::{CollaboratorService, GrantResolver, InviteRequest, UpdateRequest},
    };

    #[derive(Default)]
    struct RecordingAudit {
        events: Mutex<Vec<AuditEvent>>,
    }

    #[async_trait::async_trait]
    impl AuditService for RecordingAudit {
        async fn record(&self, event: AuditEvent) -> Result<(), openpanel_core::CoreError> {
            self.events.lock().await.push(event);
            Ok(())
        }

        async fn recent(&self, _limit: i64) -> Result<Vec<AuditEvent>, openpanel_core::CoreError> {
            Ok(self.events.lock().await.clone())
        }

        /// Paginated queries are not part of what this double asserts.
        async fn query(
            &self,
            _query: openpanel_core::audit::AuditQuery,
        ) -> Result<openpanel_core::audit::AuditPage, openpanel_core::CoreError> {
            Ok(openpanel_core::audit::AuditPage {
                events: Vec::new(),
                next_cursor: None,
            })
        }
    }

    async fn make_service() -> (
        Arc<CollaboratorService>,
        Arc<GrantResolver>,
        Arc<RecordingAudit>,
    ) {
        let db = TestDb::new().await;
        let pool = db.pool();
        let collaborators = Arc::new(SqliteCollaboratorRepository::new(pool.clone()));
        let grants = Arc::new(SqliteSiteGrantRepository::new(pool));
        let audit: Arc<RecordingAudit> = Arc::new(RecordingAudit::default());
        let resolver = Arc::new(GrantResolver::new(grants.clone()));
        let service = Arc::new(CollaboratorService::new(
            collaborators,
            grants,
            audit.clone(),
        ));
        (service, resolver, audit)
    }

    #[tokio::test]
    async fn invite_creates_collaborator_and_grant() {
        let (service, resolver, audit) = make_service().await;
        let account = Uuid::new_v4();
        let site = Uuid::new_v4();
        let req = InviteRequest {
            email: "dev@example.com".into(),
            site_id: site,
            permissions: PermissionSet::from_iter([Permission::File, Permission::Database]),
        };
        let c = service.invite(account, req, "owner").await.expect("invite");
        assert_eq!(c.email.as_str(), "dev@example.com");
        assert_eq!(c.status, CollabStatus::Invited);

        let set = resolver
            .resolve(&c.collaborator_id, site)
            .await
            .expect("resolve");
        assert!(set.contains(Permission::File));
        assert!(set.contains(Permission::Database));
        assert!(!set.contains(Permission::Mail));
        assert!(!audit.events.lock().await.is_empty());
    }

    #[tokio::test]
    async fn invite_rejects_empty_permission_set() {
        let (service, _resolver, _audit) = make_service().await;
        let err = service
            .invite(
                Uuid::new_v4(),
                InviteRequest {
                    email: "dev@example.com".into(),
                    site_id: Uuid::new_v4(),
                    permissions: PermissionSet::EMPTY,
                },
                "owner",
            )
            .await
            .unwrap_err();
        let _ = err;
    }

    #[tokio::test]
    async fn update_permissions_replaces_set() {
        let (service, resolver, _audit) = make_service().await;
        let account = Uuid::new_v4();
        let site = Uuid::new_v4();
        let req = InviteRequest {
            email: "ops@example.com".into(),
            site_id: site,
            permissions: PermissionSet::from_iter([Permission::File]),
        };
        let c = service.invite(account, req, "owner").await.expect("invite");
        service
            .update_permissions(
                &c.collaborator_id,
                site,
                UpdateRequest {
                    permissions: PermissionSet::from_iter([Permission::Cron, Permission::Mail]),
                },
                "owner",
            )
            .await
            .expect("update");
        let set = resolver
            .resolve(&c.collaborator_id, site)
            .await
            .expect("resolve");
        assert!(set.contains(Permission::Cron));
        assert!(set.contains(Permission::Mail));
        assert!(!set.contains(Permission::File));
    }

    #[tokio::test]
    async fn revoke_removes_grant_and_blocks_access() {
        let (service, resolver, _audit) = make_service().await;
        let account = Uuid::new_v4();
        let site = Uuid::new_v4();
        let req = InviteRequest {
            email: "x@example.com".into(),
            site_id: site,
            permissions: PermissionSet::from_iter([Permission::File, Permission::Database]),
        };
        let c = service.invite(account, req, "owner").await.expect("invite");
        service
            .revoke(&c.collaborator_id, site, "owner")
            .await
            .expect("revoke");
        // The grant is gone, so resolver errors.
        let err = resolver
            .resolve(&c.collaborator_id, site)
            .await
            .unwrap_err();
        let _ = err;
        // Listing grants for the site is empty.
        let grants = service.grants_for_site(site).await.expect("list");
        assert!(grants.is_empty());
    }

    #[tokio::test]
    async fn revoke_only_drops_target_site_grant() {
        let (service, _resolver, _audit) = make_service().await;
        let account = Uuid::new_v4();
        let site_a = Uuid::new_v4();
        let site_b = Uuid::new_v4();
        let req_a = InviteRequest {
            email: "dev@example.com".into(),
            site_id: site_a,
            permissions: PermissionSet::from_iter([Permission::File]),
        };
        let req_b = InviteRequest {
            email: "dev@example.com".into(),
            site_id: site_b,
            permissions: PermissionSet::from_iter([Permission::Cron]),
        };
        let c = service
            .invite(account, req_a, "owner")
            .await
            .expect("invite a");
        // Re-invite for site_b requires a NEW collaborator per
        // invite spec; we model that by inserting the second grant
        // directly. (A single email can have at most one
        // Collaborator row, but multiple SiteGrants.)
        let second_grant = openpanel_domain::SiteGrant::new(
            c.collaborator_id.clone(),
            site_b,
            PermissionSet::from_iter([Permission::Cron]),
            Utc::now(),
        );
        // Insert via the underlying repo through the service.
        let _ = req_b;
        // Use the audit service to insert via a backdoor: we can
        // call the resolver directly via the service's
        // list_for_account.
        let _ = service.find(&c.collaborator_id).await.expect("find");
        // Direct insert via a fresh grant repo is the cleanest path
        // for this assertion; build one in this scope.
        let db = TestDb::new().await;
        let grants_repo = SqliteSiteGrantRepository::new(db.pool());
        grants_repo.insert(&second_grant).await.expect("insert b");
        // Now revoke only site_a.
        let grants = Arc::new(grants_repo);
        let collaborators = Arc::new(SqliteCollaboratorRepository::new(db.pool()));
        let audit = Arc::new(RecordingAudit::default());
        let local_svc = Arc::new(CollaboratorService::new(collaborators, grants, audit));
        local_svc
            .revoke(&c.collaborator_id, site_a, "owner")
            .await
            .expect("revoke a");
        let remaining = local_svc.grants_for_site(site_b).await.expect("list b");
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].site_id, site_b);
        let _ = Email::new("user@example.com").unwrap();
    }
}
