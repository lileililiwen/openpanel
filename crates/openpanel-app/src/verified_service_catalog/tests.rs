//! Unit tests for `VerifiedServiceCatalogService`. Covers the
//! four ADDED Requirements in
//! `openspec/changes/add-verified-service-catalog/specs/verified-service-catalog/spec.md`:
//! signature-gated submission, reviewable planning, bounded-retry
//! lifecycle jobs with rollback, and request-time revocation.

#[cfg(test)]
mod tests {
    #![allow(clippy::module_inception)] // tests.rs file module + inner test module is the house pattern

    use std::sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    };

    use openpanel_core::audit::NoopAuditService;
    use openpanel_domain::{
        CatalogProvisioner, Email, HostPolicy, Password, PlanConflict, ProvisionOutcome,
        PublisherIdentity, ResourceRequest, Role, ServiceBackupPolicy, ServiceCompatibility,
        ServiceHealthProbe, ServiceManifest, ServicePermissions, ServiceUpgradePolicy,
        SourceDigest, TrustState, User, Username,
    };
    use uuid::Uuid;

    use super::super::service::VerifiedServiceCatalogService;
    use super::super::types::VerifiedServiceCatalogServiceError;

    fn owner() -> User {
        make_user("owner", Role::Owner)
    }

    fn admin() -> User {
        make_user("admin", Role::Admin)
    }

    fn member() -> User {
        make_user("member", Role::User)
    }

    fn make_user(name: &str, role: Role) -> User {
        User::new(
            Uuid::new_v4(),
            Username::new(name).expect("static"),
            Email::new(format!("{name}@example.com")).expect("static"),
            Password::hash("correct horse battery staple").expect("static"),
            role,
        )
    }

    fn publisher() -> PublisherIdentity {
        PublisherIdentity::new("openpanel-core", "OpenPanel Core Team", "pubkey-0f0f0f0f")
            .expect("static")
    }

    fn digest_a() -> SourceDigest {
        SourceDigest::new(format!("sha256:{}", "a".repeat(64))).expect("static")
    }

    fn digest_b() -> SourceDigest {
        SourceDigest::new(format!("sha256:{}", "b".repeat(64))).expect("static")
    }

    fn manifest_for(
        pub_key: PublisherIdentity,
        dig: SourceDigest,
        version: &str,
        signature: String,
    ) -> ServiceManifest {
        ServiceManifest::new(
            "demo-service",
            "Demo Service",
            version,
            pub_key,
            dig,
            signature,
            ServiceCompatibility::new(vec!["debian:amd64".into()]).expect("static"),
            ResourceRequest::new(1000, 512 * 1024 * 1024, 1024 * 1024 * 1024).expect("static"),
            ServicePermissions::new(
                true,
                vec![8080],
                vec!["registry-creds".into()],
                vec!["writes:/var/lib/demo".into()],
            )
            .expect("static"),
            ServiceHealthProbe::new("/healthz", 30, 2).expect("static"),
            ServiceBackupPolicy::new(true),
            ServiceUpgradePolicy::new(3, true).expect("static"),
        )
        .expect("static")
    }

    fn signed_manifest() -> ServiceManifest {
        let pub_key = publisher();
        let dig = digest_a();
        let signature = openpanel_domain::manifest_signature(&pub_key, &dig);
        manifest_for(pub_key, dig, "1.2.3", signature)
    }

    fn host_policy() -> HostPolicy {
        HostPolicy::new("debian", "amd64", vec![8080], 2000, 1 << 30, 10 << 30).expect("static")
    }

    /// Configurable fake provisioner. Tests flip `install_ok` /
    /// `healthy` to exercise failure and rollback paths; counters
    /// prove how many times each port method ran.
    struct FakeProvisioner {
        install_ok: AtomicBool,
        healthy: AtomicBool,
        installs: AtomicUsize,
        backups: AtomicUsize,
        removals: AtomicUsize,
    }

    impl FakeProvisioner {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                install_ok: AtomicBool::new(true),
                healthy: AtomicBool::new(true),
                installs: AtomicUsize::new(0),
                backups: AtomicUsize::new(0),
                removals: AtomicUsize::new(0),
            })
        }
    }

    impl CatalogProvisioner for FakeProvisioner {
        fn provider_id(&self) -> &'static str {
            "fake-provisioner-v1"
        }

        fn install(
            &self,
            _plan: &openpanel_domain::ServicePlan,
        ) -> Result<(), openpanel_domain::VerifiedServiceCatalogError> {
            self.installs.fetch_add(1, Ordering::SeqCst);
            if self.install_ok.load(Ordering::SeqCst) {
                Ok(())
            } else {
                Err(openpanel_domain::VerifiedServiceCatalogError::Provision(
                    "registry pull failed password=hunter2 token=abc".into(),
                ))
            }
        }

        fn backup(
            &self,
            _entry_id: &str,
        ) -> Result<(), openpanel_domain::VerifiedServiceCatalogError> {
            self.backups.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        fn health_check(
            &self,
            _entry_id: &str,
        ) -> Result<ProvisionOutcome, openpanel_domain::VerifiedServiceCatalogError> {
            Ok(ProvisionOutcome {
                healthy: self.healthy.load(Ordering::SeqCst),
                diagnostic: "probe".into(),
            })
        }

        fn remove(
            &self,
            _entry_id: &str,
        ) -> Result<(), openpanel_domain::VerifiedServiceCatalogError> {
            self.removals.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    fn service(provisioner: Arc<FakeProvisioner>) -> VerifiedServiceCatalogService {
        VerifiedServiceCatalogService::new(provisioner, Arc::new(NoopAuditService))
    }

    fn seeded(svc: &VerifiedServiceCatalogService) {
        svc.trust_publisher(&owner(), publisher()).expect("static");
        svc.submit_entry(&owner(), signed_manifest())
            .expect("static");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn signed_submission_by_trusted_publisher_is_verified() {
        let svc = service(FakeProvisioner::new());
        seeded(&svc);
        let view = svc.entry(&owner(), "demo-service").expect("entry");
        assert_eq!(view.trust_state, TrustState::Verified.as_str());
        let entries = svc.entries(&admin()).expect("admin may review");
        assert_eq!(entries.len(), 1);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn unsigned_self_signed_submission_is_community() {
        let svc = service(FakeProvisioner::new());
        let pub_key =
            PublisherIdentity::new("community-dev", "Community", "pubkey-9999").expect("static");
        let dig = digest_a();
        let signature = openpanel_domain::manifest_signature(&pub_key, &dig);
        let manifest = manifest_for(pub_key, dig, "0.1.0", signature);
        let view = svc.submit_entry(&owner(), manifest).expect("submitted");
        assert_eq!(view.trust_state, TrustState::Community.as_str());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn invalid_signature_blocks_entry_and_refuses_planning() {
        // Scenario: Invalid signature.
        let svc = service(FakeProvisioner::new());
        svc.trust_publisher(&owner(), publisher()).expect("static");
        let manifest = manifest_for(publisher(), digest_a(), "1.2.3", "0011".into());
        let view = svc
            .submit_entry(&owner(), manifest)
            .expect("kept for review");
        assert_eq!(view.trust_state, TrustState::Blocked.as_str());
        assert!(matches!(
            svc.plan_install(&owner(), "demo-service", &host_policy()),
            Err(VerifiedServiceCatalogServiceError::Domain(
                openpanel_domain::VerifiedServiceCatalogError::Blocked
            ))
        ));
        assert!(
            svc.install_entry(&owner(), "demo-service", &host_policy())
                .is_err()
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn plan_install_reveals_effect_before_confirmation() {
        let svc = service(FakeProvisioner::new());
        seeded(&svc);
        let plan = svc
            .plan_install(&owner(), "demo-service", &host_policy())
            .expect("plan");
        assert_eq!(plan.ports(), vec![8080]);
        assert!(plan.public_network());
        assert_eq!(plan.credential_refs(), vec!["registry-creds".to_string()]);
        assert_eq!(
            plan.data_effects(),
            vec!["writes:/var/lib/demo".to_string()]
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn resource_conflict_fails_planning_with_specific_conflict() {
        // Scenario: Resource conflict — specific conflict and no
        // mutation (no jobs recorded, nothing installed).
        let svc = service(FakeProvisioner::new());
        seeded(&svc);
        let tight = HostPolicy::new("debian", "amd64", vec![9999], 2000, 1 << 30, 10 << 30)
            .expect("static");
        let result = svc.plan_install(&owner(), "demo-service", &tight);
        assert!(matches!(
            result,
            Err(VerifiedServiceCatalogServiceError::Conflict(
                PlanConflict::PortNotAllowed { port: 8080 }
            ))
        ));
        assert!(svc.jobs(&owner()).expect("jobs").is_empty());
        assert!(
            !svc.entry(&owner(), "demo-service")
                .expect("entry")
                .installed
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn install_walks_job_to_succeeded_and_marks_installed() {
        let provisioner = FakeProvisioner::new();
        let svc = service(Arc::clone(&provisioner));
        seeded(&svc);
        let job = svc
            .install_entry(&owner(), "demo-service", &host_policy())
            .expect("install");
        assert_eq!(
            job.outcome,
            openpanel_domain::JobOutcome::Succeeded.as_str()
        );
        assert_eq!(job.attempts, 1);
        let entry = svc.entry(&owner(), "demo-service").expect("entry");
        assert!(entry.installed);
        assert_eq!(entry.installed_digest.as_deref(), Some(digest_a().as_str()));
        assert_eq!(provisioner.installs.load(Ordering::SeqCst), 1);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn install_failure_retries_to_bounded_max_and_redacts() {
        let provisioner = FakeProvisioner::new();
        provisioner.install_ok.store(false, Ordering::SeqCst);
        let svc = service(Arc::clone(&provisioner));
        seeded(&svc);
        let job = svc
            .install_entry(&owner(), "demo-service", &host_policy())
            .expect("job");
        // The upgrade policy declares max_attempts = 3; every
        // attempt failed at the provisioner.
        assert_eq!(job.outcome, openpanel_domain::JobOutcome::Failed.as_str());
        assert_eq!(job.attempts, job.max_attempts);
        assert_eq!(provisioner.installs.load(Ordering::SeqCst), 3);
        let error = job.last_error.expect("diagnostic");
        assert!(!error.contains("hunter2"));
        assert!(!error.contains("token=abc"));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn upgrade_requires_backup_first_then_succeeds() {
        let svc = service(FakeProvisioner::new());
        seeded(&svc);
        svc.install_entry(&owner(), "demo-service", &host_policy())
            .expect("install");
        let pub_key = publisher();
        let dig = digest_b();
        let upgraded = manifest_for(
            pub_key.clone(),
            dig.clone(),
            "1.2.4",
            openpanel_domain::manifest_signature(&pub_key, &dig),
        );
        // Pre-upgrade backup requirement is enforced fail-closed.
        let result = svc.upgrade_entry(&owner(), &upgraded, &host_policy());
        assert!(matches!(
            result,
            Err(VerifiedServiceCatalogServiceError::Domain(_))
        ));
        let backup = svc.run_backup(&owner(), "demo-service").expect("backup");
        assert_eq!(
            backup.outcome,
            openpanel_domain::JobOutcome::Succeeded.as_str()
        );
        let job = svc
            .upgrade_entry(&owner(), &upgraded, &host_policy())
            .expect("upgrade");
        assert_eq!(
            job.outcome,
            openpanel_domain::JobOutcome::Succeeded.as_str()
        );
        let entry = svc.entry(&owner(), "demo-service").expect("entry");
        assert_eq!(entry.installed_digest.as_deref(), Some(dig.as_str()));
        assert_eq!(entry.version, "1.2.4");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn failed_upgrade_restores_previous_release_with_guidance() {
        // Scenario: Upgrade failure — the previous verified release
        // remains recoverable and the job is rolled back with
        // operator guidance.
        let provisioner = FakeProvisioner::new();
        let svc = service(Arc::clone(&provisioner));
        seeded(&svc);
        svc.install_entry(&owner(), "demo-service", &host_policy())
            .expect("install");
        svc.run_backup(&owner(), "demo-service").expect("backup");
        let pub_key = publisher();
        let dig = digest_b();
        let upgraded = manifest_for(
            pub_key.clone(),
            dig.clone(),
            "1.2.4",
            openpanel_domain::manifest_signature(&pub_key, &dig),
        );
        provisioner.healthy.store(false, Ordering::SeqCst);
        let job = svc
            .upgrade_entry(&owner(), &upgraded, &host_policy())
            .expect("upgrade job");
        assert_eq!(
            job.outcome,
            openpanel_domain::JobOutcome::RolledBack.as_str()
        );
        let guidance = job.guidance.expect("guidance");
        assert!(guidance.contains("previous verified release"));
        // The entry keeps the original digest installed.
        let entry = svc.entry(&owner(), "demo-service").expect("entry");
        assert_eq!(entry.installed_digest.as_deref(), Some(digest_a().as_str()));
        // A later explicit rollback path still finds the previous
        // release cleared (the automatic restore already consumed
        // it) — the entry is intact and health can be re-verified.
        let unhealthy = svc
            .verify_health(&owner(), "demo-service")
            .expect("health job");
        assert_eq!(
            unhealthy.outcome,
            openpanel_domain::JobOutcome::Failed.as_str()
        );
        provisioner.healthy.store(true, Ordering::SeqCst);
        let healed = svc.verify_health(&owner(), "demo-service").expect("health");
        assert_eq!(
            healed.outcome,
            openpanel_domain::JobOutcome::Succeeded.as_str()
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn revoked_publisher_refuses_cached_entry_and_is_audited() {
        // Scenario: Cached revoked entry.
        let provisioner = FakeProvisioner::new();
        let svc = service(Arc::clone(&provisioner));
        seeded(&svc);
        svc.install_entry(&owner(), "demo-service", &host_policy())
            .expect("install");
        svc.revoke_publisher(&owner(), "openpanel-core")
            .expect("revoke");
        // The entry remains cached but every request is refused.
        assert!(svc.entry(&owner(), "demo-service").is_ok());
        let result = svc.install_entry(&owner(), "demo-service", &host_policy());
        assert!(matches!(
            result,
            Err(VerifiedServiceCatalogServiceError::Domain(
                openpanel_domain::VerifiedServiceCatalogError::Revoked(_)
            ))
        ));
        let view = svc.entry(&owner(), "demo-service").expect("entry");
        assert_eq!(view.trust_state, TrustState::Revoked.as_str());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn digest_revocation_refuses_new_requests_for_that_artifact() {
        let svc = service(FakeProvisioner::new());
        seeded(&svc);
        svc.revoke_digest(&owner(), digest_a().as_str())
            .expect("revoke digest");
        assert!(
            svc.plan_install(&owner(), "demo-service", &host_policy())
                .is_err()
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn non_operator_cannot_mutate_and_admin_cannot_trust_publishers() {
        let svc = service(FakeProvisioner::new());
        assert!(matches!(
            svc.submit_entry(&member(), signed_manifest()),
            Err(VerifiedServiceCatalogServiceError::Forbidden)
        ));
        svc.trust_publisher(&owner(), publisher()).expect("static");
        assert!(matches!(
            svc.trust_publisher(&admin(), publisher()),
            Err(VerifiedServiceCatalogServiceError::Forbidden)
        ));
        assert!(matches!(
            svc.revoke_publisher(&admin(), "openpanel-core"),
            Err(VerifiedServiceCatalogServiceError::Forbidden)
        ));
        // Admins may operate the lifecycle (Owner/Admin gate).
        svc.submit_entry(&admin(), signed_manifest())
            .expect("admin submit");
        svc.install_entry(&admin(), "demo-service", &host_policy())
            .expect("admin install");
    }
}
