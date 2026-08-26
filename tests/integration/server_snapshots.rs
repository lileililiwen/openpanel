//! Whole-server snapshot integration tests: create from a verified
//! backup run, preflight with single-use confirmation, and restore
//! delegation.

use crate::common::*;

fn bearer(token: &str) -> String {
    format!("Bearer {token}")
}

async fn owner_and_run(server: &TestServer) -> (String, String) {
    let token = server
        .bootstrap_owner("admin", "correct horse battery staple")
        .await;
    let auth = bearer(&token);
    let created = server
        .client()
        .post(format!("{}/api/v1/backups/plans", server.base_url()))
        .header("authorization", &auth)
        .json(&serde_json::json!({
            "name": "snapshot source",
            "resources": [{"kind": "panel_metadata"}],
            "schedule": "0 2 * * *",
            "timezone": "UTC",
            "retention_copies": 3
        }))
        .send()
        .await
        .expect("create plan");
    assert_eq!(created.status(), 201);
    let plan: serde_json::Value = created.json().await.expect("plan json");
    let plan_id = plan["id"].as_str().expect("plan id").to_owned();
    let run = server
        .client()
        .post(format!(
            "{}/api/v1/backups/plans/{plan_id}/run",
            server.base_url()
        ))
        .header("authorization", &auth)
        .send()
        .await
        .expect("run");
    assert_eq!(run.status(), 202);
    let run: serde_json::Value = run.json().await.expect("run json");
    (token, run["id"].as_str().expect("run id").to_owned())
}

#[tokio::test]
async fn snapshot_routes_require_authentication() {
    let server = TestServer::new().await;
    for path in [
        "/api/v1/server/snapshots".to_string(),
        format!(
            "/api/v1/server/snapshots/{}/preflight",
            uuid::Uuid::new_v4()
        ),
    ] {
        assert_eq!(
            server
                .client()
                .post(format!("{}{}", server.base_url(), path))
                .send()
                .await
                .expect("unauth")
                .status(),
            401
        );
    }
}

#[tokio::test]
async fn snapshot_create_preflight_and_single_use_restore() {
    let server = TestServer::new().await;
    let (token, run_id) = owner_and_run(&server).await;
    let auth = bearer(&token);
    let base = format!("{}/api/v1/server/snapshots", server.base_url());

    // Create a snapshot from the verified run.
    let created = server
        .client()
        .post(&base)
        .header("authorization", &auth)
        .json(&serde_json::json!({ "run_id": run_id }))
        .send()
        .await
        .expect("create snapshot");
    assert_eq!(
        created.status(),
        200,
        "{}",
        created.text().await.expect("body")
    );
    let manifest: serde_json::Value = created.json().await.expect("manifest json");
    assert_eq!(manifest["manifest_schema"], 1);
    assert!(manifest["entries"].as_array().expect("entries").len() >= 2);

    // Unknown snapshot id → 404.
    assert_eq!(
        server
            .client()
            .get(format!("{base}/{}", uuid::Uuid::new_v4()))
            .header("authorization", &auth)
            .send()
            .await
            .expect("unknown")
            .status(),
        404
    );

    // Preflight mints a confirmation token and reports readiness.
    let listed = server
        .client()
        .get(&base)
        .header("authorization", &auth)
        .send()
        .await
        .expect("list snapshots")
        .json::<serde_json::Value>()
        .await
        .expect("list json");
    let snapshot_id = listed["snapshots"][0]["id"]
        .as_str()
        .expect("snapshot id")
        .to_owned();

    let preflight = server
        .client()
        .post(format!("{base}/{snapshot_id}/preflight"))
        .header("authorization", &auth)
        .send()
        .await
        .expect("preflight");
    assert_eq!(
        preflight.status(),
        200,
        "{}",
        preflight.text().await.expect("body")
    );
    let body: serde_json::Value = preflight.json().await.expect("preflight json");
    let confirm_token = body["confirm_token"].as_str().expect("token").to_owned();
    assert_eq!(body["report"]["ready"], true);

    // Restore without the token is refused.
    assert_eq!(
        server
            .client()
            .post(format!("{base}/{snapshot_id}/restore"))
            .header("authorization", &auth)
            .json(&serde_json::json!({ "confirm_token": "", "overwrite": false }))
            .send()
            .await
            .expect("no token")
            .status(),
        422
    );

    // First restore with the token succeeds.
    let restored = server
        .client()
        .post(format!("{base}/{snapshot_id}/restore"))
        .header("authorization", &auth)
        .json(&serde_json::json!({ "confirm_token": confirm_token, "overwrite": false }))
        .send()
        .await
        .expect("restore");
    assert_eq!(
        restored.status(),
        200,
        "{}",
        restored.text().await.expect("body")
    );

    // Token replay is refused.
    assert_eq!(
        server
            .client()
            .post(format!("{base}/{snapshot_id}/restore"))
            .header("authorization", &auth)
            .json(&serde_json::json!({ "confirm_token": confirm_token, "overwrite": false }))
            .send()
            .await
            .expect("replay")
            .status(),
        422
    );
}

#[tokio::test]
async fn restore_aborts_at_tampered_entry_and_reports_applied() {
    let server = TestServer::new().await;
    let (token, run_id) = owner_and_run(&server).await;
    let auth = bearer(&token);
    let base = format!("{}/api/v1/server/snapshots", server.base_url());

    let created = server
        .client()
        .post(&base)
        .header("authorization", &auth)
        .json(&serde_json::json!({ "run_id": run_id }))
        .send()
        .await
        .expect("create snapshot");
    assert_eq!(created.status(), 200);
    created.json::<serde_json::Value>().await.expect("manifest");

    let listed = server
        .client()
        .get(&base)
        .header("authorization", &auth)
        .send()
        .await
        .expect("list")
        .json::<serde_json::Value>()
        .await
        .expect("list json");
    let snapshot_id = listed["snapshots"][0]["id"]
        .as_str()
        .expect("snapshot id")
        .to_owned();

    let preflight = server
        .client()
        .post(format!("{base}/{snapshot_id}/preflight"))
        .header("authorization", &auth)
        .send()
        .await
        .expect("preflight")
        .json::<serde_json::Value>()
        .await
        .expect("preflight json");
    let confirm_token = preflight["confirm_token"]
        .as_str()
        .expect("token")
        .to_owned();

    // Tamper with the LAST bundled entry: every earlier entry should
    // verify cleanly and count as "applied" when the restore aborts.
    let snapshot_dir =
        std::path::PathBuf::from(server.sandbox_path("snapshots")).join(&snapshot_id);
    let manifest_path = snapshot_dir.join("manifest.json");
    let manifest: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&manifest_path).expect("manifest read"))
            .expect("manifest parse");
    let entries = manifest["entries"].as_array().expect("entries");
    let victim = entries.last().expect("entry")["path"]
        .as_str()
        .expect("path")
        .to_owned();
    let artifact_path = snapshot_dir.join(&victim);
    let mut bytes = std::fs::read(&artifact_path).expect("artifact read");
    assert!(!bytes.is_empty(), "tampered entry is empty");
    let original = bytes[0];
    bytes[0] ^= 0xff;
    std::fs::write(&artifact_path, &bytes).expect("artifact write");
    assert_ne!(std::fs::read(&artifact_path).expect("reread")[0], original);

    // Restore aborts at that resource, listing what was applied.
    let owner = server
        .identity()
        .list_users()
        .await
        .expect("users")
        .into_iter()
        .find(|user| user.role() == openpanel_domain::Role::Owner)
        .expect("owner");
    let result = server
        .server_snapshots()
        .restore(
            &owner,
            uuid::Uuid::parse_str(&snapshot_id).expect("uuid"),
            &confirm_token,
            false,
        )
        .await;
    match result {
        Err(openpanel_app::ServerSnapshotError::Partial { applied, failed }) => {
            assert_eq!(failed, victim);
            assert_eq!(
                applied.len(),
                entries.len() - 1,
                "all entries before the tampered one must be applied: {applied:?}"
            );
            assert!(!applied.contains(&victim));
        }
        other => panic!("expected partial restore failure, got: {other:?}"),
    }
}

/// Local-filesystem offsite target used to hand a bundle between two
/// in-memory hosts.
struct FsTarget {
    root: std::path::PathBuf,
}

#[async_trait::async_trait]
impl openpanel_domain::offsite_backup_targets::BackupTargetAdapter for FsTarget {
    type Error = std::io::Error;

    async fn put(&self, key: &str, bytes: &[u8]) -> Result<(), Self::Error> {
        let path = self.root.join(key);
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(path, bytes).await
    }

    async fn get(&self, key: &str) -> Result<Vec<u8>, Self::Error> {
        tokio::fs::read(self.root.join(key)).await
    }

    async fn list(&self, prefix: &str) -> Result<Vec<String>, Self::Error> {
        let mut out = Vec::new();
        let mut stack = vec![self.root.clone()];
        while let Some(dir) = stack.pop() {
            let mut entries = tokio::fs::read_dir(&dir).await?;
            while let Some(entry) = entries.next_entry().await? {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else if let Ok(rel) = path.strip_prefix(&self.root) {
                    let key = rel.to_string_lossy().to_string();
                    if key.starts_with(prefix) {
                        out.push(key);
                    }
                }
            }
        }
        out.sort();
        Ok(out)
    }

    async fn delete(&self, key: &str) -> Result<(), Self::Error> {
        match tokio::fs::remove_file(self.root.join(key)).await {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e),
        }
    }

    async fn test(&self) -> Result<(), Self::Error> {
        tokio::fs::create_dir_all(&self.root).await
    }
}

/// Records committed resources and hands out deterministic ref ids.
struct RecordingTranslator {
    committed: std::sync::Mutex<Vec<(openpanel_domain::ImportedResourceKind, String)>>,
}

impl openpanel_app::SnapshotTranslator for RecordingTranslator {
    fn translate(
        &self,
        kind: openpanel_domain::ImportedResourceKind,
        source_key: &str,
        _payload: &[u8],
    ) -> Result<Option<uuid::Uuid>, String> {
        self.committed
            .lock()
            .expect("committed lock")
            .push((kind, source_key.to_string()));
        Ok(Some(uuid::Uuid::new_v4()))
    }

    fn rollback(
        &self,
        _kind: openpanel_domain::ImportedResourceKind,
        _source_key: &str,
        _ref_id: uuid::Uuid,
    ) -> Result<(), String> {
        Ok(())
    }
}

#[tokio::test]
async fn migrate_round_trip_between_hosts_via_shared_offsite_target() {
    use openpanel_app::{SnapshotImporterDriver, export_to_target, import_from_target};
    use openpanel_domain::{MigrationDriver, MigrationRunId};

    // Host A creates a snapshot from a verified backup run.
    let host_a = TestServer::new().await;
    let (token_a, run_id) = owner_and_run(&host_a).await;
    let auth_a = bearer(&token_a);
    let base_a = format!("{}/api/v1/server/snapshots", host_a.base_url());
    let created = host_a
        .client()
        .post(&base_a)
        .header("authorization", &auth_a)
        .json(&serde_json::json!({ "run_id": run_id }))
        .send()
        .await
        .expect("create");
    assert_eq!(created.status(), 200);
    let manifest_a: serde_json::Value = created.json().await.expect("manifest json");
    let listed = host_a
        .client()
        .get(&base_a)
        .header("authorization", &auth_a)
        .send()
        .await
        .expect("list")
        .json::<serde_json::Value>()
        .await
        .expect("list json");
    let snapshot_id = listed["snapshots"][0]["id"]
        .as_str()
        .expect("id")
        .to_owned();

    // Export host A's bundle to the shared offsite target.
    let offsite = tempfile::tempdir().expect("offsite dir");
    let target = FsTarget {
        root: offsite.path().to_path_buf(),
    };
    let bundle_dir = std::path::PathBuf::from(host_a.sandbox_path("snapshots")).join(&snapshot_id);
    export_to_target(&target, "migrate/bundle.tar.gz", &bundle_dir)
        .await
        .expect("export");

    // The importing side pulls the bundle back through the importer
    // driver (a fresh host would run exactly this half).
    let pulled = import_from_target(&target, "migrate/bundle.tar.gz")
        .await
        .expect("import");

    // The pulled manifest matches what host A produced.
    assert_eq!(
        pulled.manifest().entries.len(),
        manifest_a["entries"].as_array().expect("entries").len()
    );

    let driver = SnapshotImporterDriver::new(RecordingTranslator {
        committed: std::sync::Mutex::new(Vec::new()),
    });

    // Sniff recognizes the bundle.
    assert_eq!(
        driver.sniff(&pulled),
        Some(openpanel_domain::DriverKind::TarWithJsonManifest)
    );

    // Preview: one planned resource per manifest entry, same keys and
    // byte counts.
    let plan = driver.dry_run(&pulled).await.expect("plan");
    let planned: Vec<(String, u64)> = plan
        .resources()
        .iter()
        .map(|r| (r.source_key.clone(), r.bytes))
        .collect();
    let expected: Vec<(String, u64)> = pulled
        .manifest()
        .entries
        .iter()
        .map(|e| {
            (
                e.reference.clone().unwrap_or_else(|| match e.kind {
                    openpanel_domain::backups::snapshot::SnapshotEntryKind::PanelMetadata => {
                        "panel".to_string()
                    }
                    _ => e.path.clone(),
                }),
                std::fs::metadata(bundle_dir.join(&e.path))
                    .expect("artifact metadata")
                    .len(),
            )
        })
        .collect();
    assert_eq!(planned, expected);

    // Commit: every planned resource is translated with its payload.
    let imported = driver
        .run(
            &pulled,
            &plan,
            MigrationRunId::new(),
            uuid::Uuid::new_v4(),
            chrono::Utc::now(),
        )
        .await
        .expect("run");
    assert_eq!(imported.len(), plan.resources().len());
    for (resource, planned) in imported.iter().zip(plan.resources()) {
        assert_eq!(resource.source_key, planned.source_key);
        assert_eq!(resource.kind, planned.kind);
    }

    // Rollback undoes them in reverse order.
    let undone = driver
        .rollback(&imported, MigrationRunId::new(), chrono::Utc::now())
        .await
        .expect("rollback");
    assert_eq!(undone.len(), imported.len());
    assert!(undone.iter().all(|r| r.rolled_back));
    assert_eq!(
        undone.first().expect("last applied").source_key,
        imported.last().expect("last applied").source_key
    );
}

#[tokio::test]
async fn snapshot_retention_prunes_oldest_and_audits() {
    use openpanel_core::{AuditAction, Config};
    let _ = Config::default();
    let server = TestServer::new().await;
    let (token, run_id) = owner_and_run(&server).await;
    let auth = bearer(&token);
    let base = format!("{}/api/v1/server/snapshots", server.base_url());
    for _ in 0..3 {
        let created = server
            .client()
            .post(&base)
            .header("authorization", &auth)
            .json(&serde_json::json!({ "run_id": run_id }))
            .send()
            .await
            .expect("create");
        assert_eq!(created.status(), 200);
        // Ensure distinct created_at ordering between snapshots.
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    let owner = server
        .identity()
        .list_users()
        .await
        .expect("users")
        .into_iter()
        .find(|user| user.role() == openpanel_domain::Role::Owner)
        .expect("owner");
    let svc = server.server_snapshots();
    assert_eq!(svc.list().await.expect("list").len(), 3);

    // Keep two: the oldest bundle is removed and audited.
    let pruned = svc.prune_retention(&owner, 2).await.expect("prune");
    assert_eq!(pruned.len(), 1);
    let remaining = svc.list().await.expect("list after prune");
    assert_eq!(remaining.len(), 2);
    assert!(
        !remaining.iter().any(|(id, _)| *id == pruned[0]),
        "the pruned snapshot must be gone"
    );
    let events = server.audit_events().await;
    assert!(
        events
            .iter()
            .any(|event| event.action == AuditAction::SnapshotPruned),
        "pruning must be audited"
    );

    // Keeping more than exists is a no-op.
    assert_eq!(
        svc.prune_retention(&owner, 10).await.expect("no-op prune"),
        Vec::<uuid::Uuid>::new()
    );
}

#[tokio::test]
async fn snapshot_schedule_registers_cron_job() {
    let server = TestServer::new().await;
    let (_token, _run_id) = owner_and_run(&server).await;
    let owner = server
        .identity()
        .list_users()
        .await
        .expect("users")
        .into_iter()
        .find(|user| user.role() == openpanel_domain::Role::Owner)
        .expect("owner");
    let job_id = server
        .server_snapshots()
        .schedule(
            &owner,
            &server.cron(),
            std::path::Path::new(&server.sandbox_path("")),
            "nightly snapshot".into(),
            "0 2 * * *".into(),
            "UTC".into(),
            5,
        )
        .await
        .expect("schedule");
    let jobs = server.cron().list(owner.id(), true).await.expect("jobs");
    let job = jobs.iter().find(|job| job.id() == job_id).expect("job");
    assert_eq!(job.name(), "nightly snapshot");
}
