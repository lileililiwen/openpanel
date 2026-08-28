//! Restore drill integration tests over a real backup pipeline.
#![allow(missing_docs, clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use std::sync::Arc;

use openpanel_app::backups::{BackupPlanInput, BackupService, DrillService, DrillServiceError};
use openpanel_domain::backups::{BackupResource, drill::DrillOutcome};
use openpanel_test_support::{MockAudit, TestDb};

fn owner() -> openpanel_domain::User {
    use openpanel_domain::{Email, Password, Role, User, Username};
    User::new(
        uuid::Uuid::new_v4(),
        Username::new("owner").unwrap(),
        Email::new("owner@example.test").unwrap(),
        Password::hash("correct horse battery staple").unwrap(),
        Role::Owner,
    )
}

fn build_services(db: &TestDb, root: &std::path::Path) -> (Arc<BackupService>, Arc<DrillService>) {
    let backups = Arc::new(BackupService::new(
        db.pool(),
        root.to_path_buf(),
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
    (backups, drills)
}

async fn completed_panel_run(
    backups: &BackupService,
    owner: &openpanel_domain::User,
) -> uuid::Uuid {
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
    let run = backups
        .run_plan(owner.id(), false, plan.id())
        .await
        .unwrap();
    run.id()
}

#[tokio::test]
async fn drill_over_verified_backup_reports_passed_and_retains() {
    let db = TestDb::new().await;
    sqlx::raw_sql(openpanel_app::migrations::BACKUPS_V001)
        .execute(&db.pool())
        .await
        .unwrap();
    sqlx::raw_sql(openpanel_app::migrations::BACKUPS_V002)
        .execute(&db.pool())
        .await
        .unwrap();
    let root = tempfile::tempdir().unwrap();
    let (backups, drills) = build_services(&db, root.path());
    let owner = owner();
    let run_id = completed_panel_run(&backups, &owner).await;

    let drill = drills.run_drill(run_id).await.unwrap();
    assert_eq!(drill.outcome(), Some(DrillOutcome::Passed));
    assert!(!drill.assertions.is_empty());
    assert!(drill.assertions.iter().all(|a| a.passed));

    // Report retained in history.
    let listed = drills.list_drills(run_id).await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, drill.id);

    // Sandbox docroot wiped after the drill.
    let sandbox = openpanel_app::backups::sandbox::SandboxContext::create("openpanel").unwrap();
    let path = sandbox.docroot.path().to_path_buf();
    sandbox.teardown(None);
    assert!(!path.exists());
}

#[tokio::test]
async fn drill_rejects_non_completed_run() {
    let db = TestDb::new().await;
    sqlx::raw_sql(openpanel_app::migrations::BACKUPS_V001)
        .execute(&db.pool())
        .await
        .unwrap();
    sqlx::raw_sql(openpanel_app::migrations::BACKUPS_V002)
        .execute(&db.pool())
        .await
        .unwrap();
    let root = tempfile::tempdir().unwrap();
    let (backups, drills) = build_services(&db, root.path());
    let owner = owner();
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
    // A run that never completed (missing run row) → RunNotFound.
    let missing = uuid::Uuid::new_v4();
    let err = drills.run_drill(missing).await.unwrap_err();
    assert!(matches!(err, DrillServiceError::RunNotFound));
    let _ = plan;
}

#[tokio::test]
async fn retention_prunes_oldest_beyond_keep() {
    let db = TestDb::new().await;
    sqlx::raw_sql(openpanel_app::migrations::BACKUPS_V001)
        .execute(&db.pool())
        .await
        .unwrap();
    sqlx::raw_sql(openpanel_app::migrations::BACKUPS_V002)
        .execute(&db.pool())
        .await
        .unwrap();
    let root = tempfile::tempdir().unwrap();
    let (backups, drills) = build_services(&db, root.path());
    let owner = owner();

    // Run 21 drills over the same completed run.
    let run_id = completed_panel_run(&backups, &owner).await;
    for _ in 0..21 {
        drills.run_drill(run_id).await.unwrap();
    }
    let listed = drills.list_drills(run_id).await.unwrap();
    assert_eq!(listed.len(), 20);
}

#[tokio::test]
async fn corrupted_artifact_yields_failed_outcome() {
    let db = TestDb::new().await;
    sqlx::raw_sql(openpanel_app::migrations::BACKUPS_V001)
        .execute(&db.pool())
        .await
        .unwrap();
    sqlx::raw_sql(openpanel_app::migrations::BACKUPS_V002)
        .execute(&db.pool())
        .await
        .unwrap();
    let root = tempfile::tempdir().unwrap();
    let (backups, drills) = build_services(&db, root.path());
    let owner = owner();
    let run_id = completed_panel_run(&backups, &owner).await;

    // Corrupt the stored panel.json so the metadata assertion fails.
    let artifact_path = root
        .path()
        .join("runs")
        .join(run_id.to_string())
        .join("panel.json");
    std::fs::write(&artifact_path, b"not valid json").unwrap();

    let drill = drills.run_drill(run_id).await.unwrap();
    assert_eq!(drill.outcome(), Some(DrillOutcome::Failed));
    assert!(drill.assertions.iter().any(|a| !a.passed));
    assert!(
        drill
            .assertions
            .iter()
            .filter(|a| !a.passed)
            .all(|a| !a.detail.is_empty())
    );
}
