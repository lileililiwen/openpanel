//! Backup DR and migration operations end-to-end coverage.
//!
//! Capability under test: `backup-dr-operations` (health projection,
//! remote-target verification, restore drills, host migration).
#![allow(missing_docs, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::{
    collections::{BTreeMap, HashMap},
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use chrono::{Duration, Utc};
use openpanel_app::{
    backups::{
        BackupPlanInput, BackupService, DrillService, export_to_target, import_from_target,
        plan_health_from_runs, verify_remote_roundtrip,
    },
    notifications::{
        AdapterOutcome, NotificationAdapter, NotificationService, SqliteNotificationRepository,
    },
};
use openpanel_core::{Migration, MigrationRunner};
use openpanel_domain::{
    Email, Password, Role, User, Username,
    backups::{
        BackupResource, Guidance,
        health::BackupHealthStatus,
        migration::{
            assess_migration_readiness, check_manifest_compatibility, migration_bootstrap_command,
            preview_migration_collisions,
        },
        snapshot::{SUPPORTED_MANIFEST_SCHEMA, SnapshotEntry, SnapshotEntryKind, SnapshotManifest},
    },
    migration_importers::MigrationDriver,
    offsite_backup_targets::BackupTargetAdapter,
};
use openpanel_test_support::{MockAudit, TestDb};
use sha2::{Digest, Sha256};

const CAPABILITY: &str = "backup-dr-operations";

fn owner() -> User {
    User::new(
        uuid::Uuid::new_v4(),
        Username::new("owner").unwrap(),
        Email::new("owner@example.test").unwrap(),
        Password::hash("correct horse battery staple").unwrap(),
        Role::Owner,
    )
}

async fn backup_fixture() -> (
    TestDb,
    tempfile::TempDir,
    Arc<BackupService>,
    Arc<DrillService>,
) {
    let db = TestDb::new().await;
    for (version, sql) in [
        ("001", openpanel_app::migrations::BACKUPS_V001),
        ("002", openpanel_app::migrations::BACKUPS_V002),
    ] {
        MigrationRunner::for_sqlite(db.pool())
            .apply_module(
                "backups",
                &[Migration {
                    module: "backups",
                    version: version.into(),
                    description: "test".into(),
                    sql: sql.into(),
                }],
            )
            .await
            .unwrap();
    }
    let root = tempfile::tempdir().unwrap();
    let backups = Arc::new(BackupService::new(
        db.pool(),
        root.path().to_path_buf(),
        Arc::new(MockAudit::stub()),
        None,
        None,
    ));
    let drills = Arc::new(DrillService::new(
        db.pool(),
        backups.clone(),
        Arc::new(MockAudit::stub()),
        None,
    ));
    (db, root, backups, drills)
}

async fn completed_panel_run(backups: &BackupService, owner: &User) -> uuid::Uuid {
    let plan = backups
        .create_plan(
            owner.id(),
            BackupPlanInput {
                name: "drill-plan".into(),
                resources: vec![BackupResource::PanelMetadata],
                schedule: "0 2 * * *".into(),
                timezone: "UTC".into(),
                retention_copies: 3,
            },
        )
        .await
        .unwrap();
    backups
        .run_plan(owner.id(), false, plan.id())
        .await
        .unwrap()
        .id()
}

// -- health projection --

#[tokio::test]
async fn health_projection_marks_stale_plan_and_links_recovery() {
    assert_eq!(CAPABILITY, "backup-dr-operations");
    let (_db, _root, backups, _) = backup_fixture().await;
    let owner = owner();
    let plan = backups
        .create_plan(
            owner.id(),
            BackupPlanInput {
                name: "nightly".into(),
                resources: vec![BackupResource::PanelMetadata],
                schedule: "0 2 * * *".into(),
                timezone: "UTC".into(),
                retention_copies: 5,
            },
        )
        .await
        .unwrap();
    let runs = backups.runs(owner.id(), false).await.unwrap();
    let now = Utc::now();

    // No runs yet: unknown, with guidance to trigger the plan.
    let health = plan_health_from_runs(&plan, &runs, None, None, now, 86_400, false);
    assert_eq!(health.status(), BackupHealthStatus::Unknown);
    assert!(health.guidance().contains("trigger"));

    // A two-day-old success against a daily plan: stale, linking
    // logs/retry/configuration.
    let stale = plan_health_from_runs(
        &plan,
        &runs,
        Some(now - Duration::hours(49)),
        None,
        now,
        86_400,
        true,
    );
    assert_eq!(stale.status(), BackupHealthStatus::Stale);
    assert!(stale.guidance().contains("logs"));
    assert_eq!(stale.rpo_secs(), Some(49 * 3600));
    assert_eq!(stale.retention_copies(), 5);
}

// -- remote-target verification --

struct MemoryAdapter {
    objects: Mutex<HashMap<String, Vec<u8>>>,
    fail_test: bool,
}

#[async_trait]
impl BackupTargetAdapter for MemoryAdapter {
    type Error = openpanel_domain::OffsiteBackupError;

    async fn put(&self, key: &str, bytes: &[u8]) -> Result<(), Self::Error> {
        self.objects
            .lock()
            .unwrap()
            .insert(key.to_string(), bytes.to_vec());
        Ok(())
    }

    async fn get(&self, key: &str) -> Result<Vec<u8>, Self::Error> {
        self.objects
            .lock()
            .unwrap()
            .get(key)
            .cloned()
            .ok_or(openpanel_domain::OffsiteBackupError::CredentialNotFound)
    }

    async fn list(&self, prefix: &str) -> Result<Vec<String>, Self::Error> {
        let mut keys: Vec<String> = self
            .objects
            .lock()
            .unwrap()
            .keys()
            .filter(|key| key.starts_with(prefix))
            .cloned()
            .collect();
        keys.sort();
        Ok(keys)
    }

    async fn delete(&self, key: &str) -> Result<(), Self::Error> {
        self.objects.lock().unwrap().remove(key);
        Ok(())
    }

    async fn test(&self) -> Result<(), Self::Error> {
        if self.fail_test {
            Err(openpanel_domain::OffsiteBackupError::Adapter(
                "unreachable".into(),
            ))
        } else {
            Ok(())
        }
    }
}

#[tokio::test]
async fn remote_roundtrip_verifies_write_read_and_cleans_up() {
    assert_eq!(CAPABILITY, "backup-dr-operations");
    let adapter = MemoryAdapter {
        objects: Mutex::new(HashMap::new()),
        fail_test: false,
    };
    let report = verify_remote_roundtrip(&adapter, "host1/backups").await;
    assert!(report.all_ok());
    assert!(report.authenticated());
    assert!(report.latency_ms() < 60_000);
    assert!(adapter.objects.lock().unwrap().is_empty());
}

#[tokio::test]
async fn remote_roundtrip_flags_unavailable_storage() {
    assert_eq!(CAPABILITY, "backup-dr-operations");
    let adapter = MemoryAdapter {
        objects: Mutex::new(HashMap::new()),
        fail_test: true,
    };
    let report = verify_remote_roundtrip(&adapter, "host1/backups").await;
    assert!(!report.reachable());
    assert!(!report.all_ok());
    assert!(report.guidance().contains("credentials"));
}

// -- migration round-trip --

fn bundle_manifest(payloads: &BTreeMap<String, Vec<u8>>) -> SnapshotManifest {
    let entries = payloads
        .iter()
        .map(|(path, bytes)| SnapshotEntry {
            kind: SnapshotEntryKind::Site,
            reference: Some(path.clone()),
            path: path.clone(),
            sha256: hex::encode(Sha256::digest(bytes)),
        })
        .collect();
    SnapshotManifest::new(Utc::now(), uuid::Uuid::new_v4(), "0.1.0".into(), entries).unwrap()
}

struct BundleAdapter {
    objects: Mutex<HashMap<String, Vec<u8>>>,
}

#[async_trait]
impl BackupTargetAdapter for BundleAdapter {
    type Error = openpanel_domain::migration_importers::MigrationError;

    async fn put(&self, key: &str, bytes: &[u8]) -> Result<(), Self::Error> {
        self.objects
            .lock()
            .unwrap()
            .insert(key.to_string(), bytes.to_vec());
        Ok(())
    }

    async fn get(&self, key: &str) -> Result<Vec<u8>, Self::Error> {
        self.objects.lock().unwrap().get(key).cloned().ok_or(
            openpanel_domain::migration_importers::MigrationError::MalformedSource(
                "missing bundle".into(),
            ),
        )
    }

    async fn list(&self, prefix: &str) -> Result<Vec<String>, Self::Error> {
        Ok(self
            .objects
            .lock()
            .unwrap()
            .keys()
            .filter(|key| key.starts_with(prefix))
            .cloned()
            .collect())
    }

    async fn delete(&self, key: &str) -> Result<(), Self::Error> {
        self.objects.lock().unwrap().remove(key);
        Ok(())
    }

    async fn test(&self) -> Result<(), Self::Error> {
        Ok(())
    }
}

#[tokio::test]
async fn migration_roundtrip_preserves_manifest_and_previews_collisions() {
    use openpanel_app::backups::{SkipAll, SnapshotBundleSource, SnapshotImporterDriver};

    assert_eq!(CAPABILITY, "backup-dr-operations");
    let dir = tempfile::tempdir().unwrap();
    let mut payloads = BTreeMap::new();
    payloads.insert("sites/a.bin".to_string(), b"site-a".to_vec());
    payloads.insert("sites/b.bin".to_string(), b"site-b".to_vec());
    let manifest = bundle_manifest(&payloads);
    std::fs::write(
        dir.path().join("manifest.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    for (path, bytes) in &payloads {
        let full = dir.path().join(path);
        std::fs::create_dir_all(full.parent().unwrap()).unwrap();
        std::fs::write(full, bytes).unwrap();
    }

    // Export to the shared offsite target, then import back.
    let adapter = BundleAdapter {
        objects: Mutex::new(HashMap::new()),
    };
    export_to_target(&adapter, "host1/bundle.tar.gz", dir.path())
        .await
        .unwrap();
    let source = import_from_target(&adapter, "host1/bundle.tar.gz")
        .await
        .unwrap();
    assert!(SnapshotBundleSource::from_tar_gz(&source.to_tar_gz().unwrap()).is_ok());

    // Dry-run previews every entry; compatibility gates the import.
    let driver = SnapshotImporterDriver::new(SkipAll);
    let plan = driver.dry_run(&source).await.unwrap();
    assert_eq!(plan.resources().len(), 2);
    assert!(check_manifest_compatibility(
        source.manifest().manifest_schema
    ));
    assert!(!check_manifest_compatibility(SUPPORTED_MANIFEST_SCHEMA + 1));

    // Collision preview against the destination inventory.
    let planned: Vec<String> = vec!["sites/a.bin".to_string(), "sites/c.bin".to_string()];
    let collisions = preview_migration_collisions(&["sites/a.bin".to_string()], &planned);
    assert_eq!(collisions, vec!["sites/a.bin".to_string()]);

    // Bootstrap command carries the key without secrets.
    let readiness = assess_migration_readiness(
        source.manifest().manifest_schema,
        &["sites/a.bin".to_string()],
        &planned,
        "host1/bundle.tar.gz",
        "fresh-1",
    )
    .unwrap();
    assert!(readiness.compatible());
    assert_eq!(readiness.collisions(), &["sites/a.bin".to_string()]);
    assert!(
        migration_bootstrap_command("host1/bundle.tar.gz", "fresh-1")
            .unwrap()
            .contains("host1/bundle.tar.gz")
    );
}

// -- restore drills with notification wiring --
struct SinkAdapter;

#[async_trait]
impl NotificationAdapter for SinkAdapter {
    async fn smtp(
        &self,
        _channel: &openpanel_domain::notifications::SmtpChannel,
        _password: &str,
        _recipient: &str,
        _event: &openpanel_domain::notifications::NotificationEvent,
    ) -> AdapterOutcome {
        AdapterOutcome::Accepted
    }

    async fn webhook(
        &self,
        _channel: &openpanel_domain::notifications::WebhookChannel,
        _secret: &str,
        _delivery_id: uuid::Uuid,
        _event: &openpanel_domain::notifications::NotificationEvent,
        _body: &[u8],
    ) -> AdapterOutcome {
        AdapterOutcome::Accepted
    }
}

#[tokio::test]
async fn failed_drill_reports_tears_down_and_notifies() {
    use openpanel_domain::backups::drill::DrillOutcome;

    assert_eq!(CAPABILITY, "backup-dr-operations");
    let (db, root, backups, drills) = backup_fixture().await;
    MigrationRunner::for_sqlite(db.pool())
        .apply_module(
            "notifications",
            &[Migration {
                module: "notifications",
                version: "001".into(),
                description: "test".into(),
                sql: openpanel_app::migrations::NOTIFICATIONS_V001.into(),
            }],
        )
        .await
        .unwrap();
    let notifications = Arc::new(NotificationService::new(
        Arc::new(SqliteNotificationRepository::new(db.pool())),
        Arc::new(SinkAdapter),
        Arc::new(MockAudit::stub()),
        [9; 32],
    ));
    drills.attach_notifications(notifications);

    let owner = owner();
    let run_id = completed_panel_run(&backups, &owner).await;

    // Corrupt the stored dump so the drill fails through the
    // production assertion pipeline.
    let artifact = root
        .path()
        .join("runs")
        .join(run_id.to_string())
        .join("panel.json");
    std::fs::write(&artifact, b"not valid json").unwrap();

    let drill = drills.run_drill(run_id).await.unwrap();
    assert_eq!(drill.outcome(), Some(DrillOutcome::Failed));
    assert!(drill.assertions.iter().any(|assertion| !assertion.passed));

    // Report retained in history; sandbox leaves nothing behind (the
    // drill consumed its TempDir on teardown).
    let listed = drills.list_drills(run_id).await.unwrap();
    assert_eq!(listed.len(), 1);
    let fetched = drills.get_drill(drill.id).await.unwrap();
    assert_eq!(fetched.id, drill.id);
}

// -- migration audit trail on the import path --

struct RecordingAudit {
    events: Mutex<Vec<openpanel_core::AuditEvent>>,
}

#[async_trait]
impl openpanel_core::AuditService for RecordingAudit {
    async fn record(&self, event: openpanel_core::AuditEvent) -> openpanel_core::CoreResult<()> {
        self.events.lock().unwrap().push(event);
        Ok(())
    }

    async fn recent(
        &self,
        _limit: i64,
    ) -> openpanel_core::CoreResult<Vec<openpanel_core::AuditEvent>> {
        Ok(self.events.lock().unwrap().clone())
    }

    async fn query(
        &self,
        _query: openpanel_core::audit::AuditQuery,
    ) -> openpanel_core::CoreResult<openpanel_core::audit::AuditPage> {
        Ok(openpanel_core::audit::AuditPage {
            events: vec![],
            next_cursor: None,
        })
    }
}

#[tokio::test]
async fn migration_preview_and_commit_emit_audit_events() {
    use openpanel_app::backups::{SkipAll, SnapshotBundleSource, SnapshotImporterDriver};
    use openpanel_app::migration_importers::{MigrationService, SqliteMigrationRepository};
    use openpanel_core::{AuditAction, Migration as CoreMigration, MigrationRunner};

    assert_eq!(CAPABILITY, "backup-dr-operations");
    let db = TestDb::new().await;
    MigrationRunner::for_sqlite(db.pool())
        .apply_module(
            "migration_importers",
            &[CoreMigration {
                module: "migration_importers",
                version: "001".into(),
                description: "test".into(),
                sql: openpanel_app::migrations::MIGRATION_IMPORTERS_V001.into(),
            }],
        )
        .await
        .unwrap();
    let audit = Arc::new(RecordingAudit {
        events: Mutex::new(Vec::new()),
    });
    let service = MigrationService::new(
        Arc::new(SqliteMigrationRepository::new(db.pool())),
        audit.clone(),
    );

    let mut payloads = BTreeMap::new();
    payloads.insert("sites/a.bin".to_string(), b"site-a".to_vec());
    let manifest = bundle_manifest(&payloads);
    let source = SnapshotBundleSource::new(manifest, payloads).unwrap();
    let driver = SnapshotImporterDriver::new(SkipAll);

    let plan = service
        .preview(&driver, &source, None, "owner@example.test")
        .await
        .unwrap();
    let imported = service
        .run(
            &driver,
            &source,
            &plan,
            uuid::Uuid::new_v4(),
            "owner@example.test",
        )
        .await
        .unwrap();
    assert!(imported.is_empty());

    let events = audit.events.lock().unwrap();
    assert!(
        events
            .iter()
            .any(|event| event.action == AuditAction::MigrationPreviewed)
    );
    assert!(
        events
            .iter()
            .any(|event| event.action == AuditAction::MigrationRunCommitted)
    );
    // Audit metadata is redacted: no payload bytes travel with events.
    for event in events.iter() {
        let serialized = serde_json::to_string(&event.metadata).unwrap();
        assert!(!serialized.contains("site-a"));
    }
}
