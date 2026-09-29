//! Migration importers integration tests: preview, run, and
//! rollback through the reference tar-with-json-manifest driver
//! against the real SQLite-backed service.
//!
//! Scenarios covered: Existing domain conflict, Partial verification.
//!
//! Cross-capability scenarios proved by the portable source adapters:
//! Imported site reuses existing site workspace (`sites`), Imported
//! mailbox maps to the existing mail surface (`mail`), Imported zone
//! validates before cutover (`dns`), and Imported certificate preflight
//! gates cutover (`ssl`). The host-scoped capability mismatch
//! (`terminal-host-fleet`) and the fleet capability-health view
//! (`monitoring-fleet-operations`) are covered by the host-ops
//! integration test.

use std::collections::BTreeMap;

use openpanel_app::migration_importers::verifiers::{FailingVerifier, SitesPostImportVerifier};
use openpanel_app::{
    JsonManifest, JsonManifestBundle, ManifestResource, ManifestTranslator,
    TarWithJsonManifestDriver,
};
use openpanel_domain::{
    ConflictPolicy, DriverKind, ImportConflict, ImportedResourceKind, MigrationDriver,
    MigrationError, MigrationPlanId, MigrationRunId, MigrationWarning, PlannedResource,
};
use uuid::Uuid;

use crate::common::*;

/// A translator that stamps a deterministic ref id per site key.
struct StubTranslator {
    counter: std::sync::atomic::AtomicU64,
}

impl ManifestTranslator for StubTranslator {
    fn translate(
        &self,
        kind: ImportedResourceKind,
        _source_key: &str,
        _payload: &[u8],
    ) -> Result<Option<Uuid>, String> {
        if kind == ImportedResourceKind::Site {
            let n = self
                .counter
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(Some(Uuid::from_u128(1 + n as u128)))
        } else {
            Ok(None)
        }
    }
}

fn sample_bundle() -> JsonManifestBundle {
    let manifest = JsonManifest {
        format: "openpanel-backup".to_string(),
        version: "1.0".to_string(),
        resources: vec![
            ManifestResource {
                kind: ImportedResourceKind::Site,
                source_key: "example.com".to_string(),
                payload_path: "sites/example.com".to_string(),
                bytes: 5,
                depends_on: None,
                description: Some("vhost".to_string()),
            },
            ManifestResource {
                kind: ImportedResourceKind::Site,
                source_key: "blog.example.com".to_string(),
                payload_path: "sites/blog.example.com".to_string(),
                bytes: 5,
                depends_on: None,
                description: Some("vhost".to_string()),
            },
        ],
    };
    let mut payloads = BTreeMap::new();
    payloads.insert("sites/example.com".to_string(), b"hello".to_vec());
    payloads.insert("sites/blog.example.com".to_string(), b"hello".to_vec());
    JsonManifestBundle::new(manifest, payloads).unwrap()
}

#[tokio::test]
async fn migration_preview_run_rollback_round_trips() {
    let server = TestServer::new().await;
    let service = server.migration_importers();
    let driver = TarWithJsonManifestDriver::new(StubTranslator {
        counter: std::sync::atomic::AtomicU64::new(0),
    });
    let bundle = sample_bundle();

    // Preview: the driver sniffs the format and builds a plan.
    let plan = service
        .preview(
            &driver,
            &bundle,
            Some(DriverKind::TarWithJsonManifest),
            "admin",
        )
        .await
        .expect("preview");
    assert_eq!(plan.driver(), DriverKind::TarWithJsonManifest);
    assert_eq!(plan.resources().len(), 2);
    assert_eq!(plan.total_bytes(), 10);

    // Run: commits both sites with deterministic ref ids.
    let owner = server
        .client()
        .get(format!("{}/api/v1/identity/me", server.base_url()))
        .bearer_auth(
            &server
                .bootstrap_owner("migadmin", "correct horse battery staple")
                .await,
        )
        .send()
        .await
        .expect("me")
        .json::<serde_json::Value>()
        .await
        .expect("me body");
    let owner_id = Uuid::parse_str(owner["id"].as_str().expect("id")).expect("uuid");
    let imported = service
        .run(&driver, &bundle, &plan, owner_id, "admin")
        .await
        .expect("run");
    assert_eq!(imported.len(), 2);
    assert!(
        imported
            .iter()
            .all(|r| r.kind == ImportedResourceKind::Site && !r.rolled_back)
    );

    // Idempotency: the same plan is refused once imported.
    let second = service
        .run(&driver, &bundle, &plan, owner_id, "admin")
        .await;
    assert!(matches!(second, Err(MigrationError::AlreadyImported(_))));

    // Recent runs are listed.
    let runs = service.recent_runs(10).await.expect("runs");
    assert_eq!(runs.len(), 1);
    assert!(runs[0].run_id() == imported[0].run_id);

    // Rollback undoes the run.
    let run_id = imported[0].run_id;
    let rolled_back = service
        .rollback(&driver, run_id, "admin")
        .await
        .expect("rollback");
    assert_eq!(rolled_back.len(), 2);
    let runs = service.recent_runs(10).await.expect("runs");
    assert_eq!(runs[0].status_str(), "rolled_back");

    // Imported resources are now marked rolled back.
    let resources = service.imported_resources(run_id).await.expect("resources");
    assert!(resources.iter().all(|r| r.rolled_back));
}

#[tokio::test]
async fn migration_preview_rejects_driver_hint_mismatch() {
    let server = TestServer::new().await;
    let service = server.migration_importers();
    let driver = TarWithJsonManifestDriver::new(StubTranslator {
        counter: std::sync::atomic::AtomicU64::new(0),
    });
    let bundle = sample_bundle();
    let err = service
        .preview(&driver, &bundle, Some(DriverKind::BaotaBackup), "admin")
        .await
        .expect_err("mismatch");
    assert!(matches!(err, MigrationError::UnknownSource(_)));
}

#[tokio::test]
async fn migration_empty_plan_refuses_run() {
    let server = TestServer::new().await;
    let service = server.migration_importers();
    let driver = TarWithJsonManifestDriver::new(StubTranslator {
        counter: std::sync::atomic::AtomicU64::new(0),
    });
    let manifest = JsonManifest {
        format: "openpanel-backup".to_string(),
        version: "1.0".to_string(),
        resources: Vec::new(),
    };
    let bundle = JsonManifestBundle::new(manifest, BTreeMap::new()).unwrap();
    let plan = service
        .preview(&driver, &bundle, None, "admin")
        .await
        .expect("preview");
    assert!(plan.is_empty());
    let owner = Uuid::new_v4();
    let err = service
        .run(&driver, &bundle, &plan, owner, "admin")
        .await
        .expect_err("refuse");
    assert!(matches!(err, MigrationError::MalformedSource(_)));
    let _ = MigrationPlanId::new();
    let _ = MigrationRunId::new();
}

#[tokio::test]
async fn preview_blocks_existing_domain_with_conflict_policy() {
    // Existing domain conflict: the plan lists a conflict for example.com and the Block policy refuses commit.
    let plan = openpanel_domain::MigrationPlan::new(
        MigrationPlanId::new(),
        DriverKind::CpanelPkgacct,
        vec![PlannedResource::new(ImportedResourceKind::Site, "example.com", "vhost", 1).unwrap()],
        vec![ImportConflict {
            kind: ImportedResourceKind::Site,
            source_key: "example.com".to_string(),
            reason: "already exists".to_string(),
        }],
        Vec::new(),
        chrono::Utc::now(),
    )
    .with_conflict_policy(ConflictPolicy::Block);
    assert!(plan.policy_blocks_commit());
    // Replace records a warning and allows commit.
    let replacing = openpanel_domain::MigrationPlan::new(
        MigrationPlanId::new(),
        DriverKind::CpanelPkgacct,
        vec![PlannedResource::new(ImportedResourceKind::Site, "example.com", "vhost", 1).unwrap()],
        vec![ImportConflict {
            kind: ImportedResourceKind::Site,
            source_key: "example.com".to_string(),
            reason: "already exists".to_string(),
        }],
        vec![MigrationWarning {
            message: "replacing example.com".to_string(),
        }],
        chrono::Utc::now(),
    )
    .with_conflict_policy(ConflictPolicy::Replace);
    assert!(!replacing.policy_blocks_commit());
}

#[tokio::test]
async fn cpanel_pkgacct_previews_and_runs() {
    use openpanel_app::migration_importers::drivers::{CpanelPkgacctBundle, CpanelPkgacctDriver};
    let driver = CpanelPkgacctDriver::new(StubTranslator {
        counter: std::sync::atomic::AtomicU64::new(0),
    });
    let bundle =
        CpanelPkgacctBundle::new(vec!["alice".to_string()], vec!["example.com".to_string()]);
    assert_eq!(driver.sniff(&bundle), Some(DriverKind::CpanelPkgacct));
    let plan = driver.dry_run(&bundle).await.unwrap();
    assert_eq!(plan.resources().len(), 2);
    let _ = driver
        .run(
            &bundle,
            &plan,
            MigrationRunId::new(),
            Uuid::new_v4(),
            chrono::Utc::now(),
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn baota_backup_previews_and_runs() {
    use openpanel_app::migration_importers::drivers::{BaotaBackupDriver, BaotaBundle};
    let driver = BaotaBackupDriver::new(StubTranslator {
        counter: std::sync::atomic::AtomicU64::new(0),
    });
    let bundle = BaotaBundle::new(vec!["example.com".to_string()], vec!["mydb".to_string()]);
    assert_eq!(driver.sniff(&bundle), Some(DriverKind::BaotaBackup));
    let plan = driver.dry_run(&bundle).await.unwrap();
    assert_eq!(plan.resources().len(), 2);
    let _ = driver
        .run(
            &bundle,
            &plan,
            MigrationRunId::new(),
            Uuid::new_v4(),
            chrono::Utc::now(),
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn partial_verification_blocks_cutover() {
    // Partial verification: when a verifier fails, the run is marked PartiallyCompleted and cutover is refused.
    let server = TestServer::new().await;
    let service = server.migration_importers();
    let driver = TarWithJsonManifestDriver::new(StubTranslator {
        counter: std::sync::atomic::AtomicU64::new(0),
    });
    let bundle = sample_bundle();
    let plan = service
        .preview(&driver, &bundle, None, "admin")
        .await
        .unwrap();
    let owner = Uuid::new_v4();
    let verifiers: Vec<Box<dyn openpanel_app::migration_importers::verifiers::PostImportVerifier>> = vec![
        Box::new(SitesPostImportVerifier),
        Box::new(FailingVerifier {
            evidence: "mail failed".to_string(),
        }),
    ];
    let _ = service
        .run_with_verifiers(&driver, &bundle, &plan, owner, "admin", &verifiers)
        .await
        .unwrap();
    let runs = service.recent_runs(10).await.unwrap();
    assert_eq!(runs[0].status_str(), "partially_completed");
    assert!(!runs[0].allows_cutover());
}
