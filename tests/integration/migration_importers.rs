//! Migration importers integration tests: preview, run, and
//! rollback through the reference tar-with-json-manifest driver
//! against the real SQLite-backed service.

use std::collections::BTreeMap;

use openpanel_app::{
    JsonManifest, JsonManifestBundle, ManifestResource, ManifestTranslator,
    TarWithJsonManifestDriver,
};
use openpanel_domain::{
    DriverKind, ImportedResourceKind, MigrationError, MigrationPlanId, MigrationRunId,
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
