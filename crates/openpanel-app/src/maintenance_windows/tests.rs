//! Scheduled maintenance windows bounded context unit and service tests.

use std::sync::Arc;

use chrono::{Duration, Utc};
use openpanel_core::NoopAuditService;
use openpanel_domain::{
    DestructiveActionClass, MaintenanceRepository, MaintenanceWindow, Role,
};
use openpanel_test_support::TestDb;
use uuid::Uuid;

use crate::maintenance_windows::{MaintenanceEnforcer, SqliteMaintenanceRepository};

fn admin_user() -> openpanel_domain::User {
    use openpanel_domain::{Email, Password, Username};
    openpanel_domain::User::new(
        Uuid::new_v4(),
        Username::new("admin").expect("static"),
        Email::new("admin@example.com").expect("static"),
        Password::hash("correct horse battery staple").expect("static"),
        Role::Admin,
    )
}

fn non_admin_user() -> openpanel_domain::User {
    use openpanel_domain::{Email, Password, Username};
    openpanel_domain::User::new(
        Uuid::new_v4(),
        Username::new("viewer").expect("static"),
        Email::new("viewer@example.com").expect("static"),
        Password::hash("correct horse battery staple").expect("static"),
        Role::User,
    )
}

fn window(starts_in: i64, lasts: i64, class: DestructiveActionClass) -> MaintenanceWindow {
    let now = Utc::now();
    MaintenanceWindow {
        id: Uuid::new_v4(),
        label: "test".into(),
        starts_at: now + Duration::seconds(starts_in),
        ends_at: now + Duration::seconds(starts_in + lasts),
        blocked_classes: vec![class],
        created_at: now,
        created_by: Uuid::new_v4(),
    }
}

#[tokio::test]
async fn schedule_persists_window() {
    let db = TestDb::new().await;
    let repo = Arc::new(SqliteMaintenanceRepository::new(db.pool()));
    let enforcer = MaintenanceEnforcer::new(repo.clone(), Arc::new(NoopAuditService));
    let caller = admin_user();
    let w = window(-60, 300, DestructiveActionClass::PackageInstall);
    enforcer.schedule(&caller, w.clone()).await.expect("schedule");
    let list = repo.list_windows().await.expect("list");
    assert_eq!(list.len(), 1);
}

#[tokio::test]
async fn schedule_rejects_overlapping_window() {
    let db = TestDb::new().await;
    let repo = Arc::new(SqliteMaintenanceRepository::new(db.pool()));
    let enforcer = MaintenanceEnforcer::new(repo, Arc::new(NoopAuditService));
    let caller = admin_user();
    let first = window(60, 600, DestructiveActionClass::PackageInstall);
    enforcer.schedule(&caller, first.clone()).await.expect("first");
    let second = window(120, 300, DestructiveActionClass::PackageInstall);
    let res = enforcer.schedule(&caller, second).await;
    assert!(matches!(res, Err(openpanel_domain::MaintenanceError::WindowOverlap)));
}

#[tokio::test]
async fn gate_blocks_destructive_action_during_window() {
    let db = TestDb::new().await;
    let repo = Arc::new(SqliteMaintenanceRepository::new(db.pool()));
    let enforcer = MaintenanceEnforcer::new(repo, Arc::new(NoopAuditService));
    let caller = admin_user();
    enforcer
        .schedule(&caller, window(-60, 300, DestructiveActionClass::PackageInstall))
        .await
        .expect("schedule");
    let res = enforcer
        .gate(&caller, DestructiveActionClass::PackageInstall, None)
        .await;
    assert!(matches!(
        res,
        Err(openpanel_domain::MaintenanceError::DestructiveBlocked)
    ));
}

#[tokio::test]
async fn gate_allows_destructive_action_outside_window() {
    let db = TestDb::new().await;
    let repo = Arc::new(SqliteMaintenanceRepository::new(db.pool()));
    let enforcer = MaintenanceEnforcer::new(repo, Arc::new(NoopAuditService));
    let caller = admin_user();
    enforcer
        .schedule(&caller, window(60, 600, DestructiveActionClass::PackageInstall))
        .await
        .expect("schedule");
    let res = enforcer
        .gate(&caller, DestructiveActionClass::PackageInstall, None)
        .await;
    assert!(res.is_ok());
}

#[tokio::test]
async fn gate_accepts_valid_override_and_consumes_it_once() {
    let db = TestDb::new().await;
    let repo = Arc::new(SqliteMaintenanceRepository::new(db.pool()));
    let enforcer = MaintenanceEnforcer::new(repo, Arc::new(NoopAuditService));
    let caller = admin_user();
    enforcer
        .schedule(&caller, window(-60, 300, DestructiveActionClass::PackageInstall))
        .await
        .expect("schedule");
    let override_ = enforcer
        .issue_override(&caller, DestructiveActionClass::PackageInstall, "fix", 60)
        .await
        .expect("issue");
    assert!(enforcer
        .gate(&caller, DestructiveActionClass::PackageInstall, Some(override_.id))
        .await
        .is_ok());
    // Second call must be rejected: override is consumed.
    let res = enforcer
        .gate(&caller, DestructiveActionClass::PackageInstall, Some(override_.id))
        .await;
    assert!(matches!(
        res,
        Err(openpanel_domain::MaintenanceError::OverrideExpired)
            | Err(openpanel_domain::MaintenanceError::OverrideMissing)
    ));
}

#[tokio::test]
async fn gate_rejects_wrong_class_override() {
    let db = TestDb::new().await;
    let repo = Arc::new(SqliteMaintenanceRepository::new(db.pool()));
    let enforcer = MaintenanceEnforcer::new(repo, Arc::new(NoopAuditService));
    let caller = admin_user();
    enforcer
        .schedule(&caller, window(-60, 300, DestructiveActionClass::PackageInstall))
        .await
        .expect("schedule");
    let override_ = enforcer
        .issue_override(&caller, DestructiveActionClass::SchemaMigration, "fix", 60)
        .await
        .expect("issue");
    let res = enforcer
        .gate(&caller, DestructiveActionClass::PackageInstall, Some(override_.id))
        .await;
    assert!(matches!(
        res,
        Err(openpanel_domain::MaintenanceError::OverrideMissing)
    ));
}

#[tokio::test]
async fn non_admin_cannot_schedule() {
    let db = TestDb::new().await;
    let repo = Arc::new(SqliteMaintenanceRepository::new(db.pool()));
    let enforcer = MaintenanceEnforcer::new(repo, Arc::new(NoopAuditService));
    let user = non_admin_user();
    let res = enforcer
        .schedule(&user, window(-60, 300, DestructiveActionClass::PackageInstall))
        .await;
    assert!(matches!(
        res,
        Err(openpanel_domain::MaintenanceError::Forbidden)
    ));
}

#[tokio::test]
async fn cancel_removes_window() {
    let db = TestDb::new().await;
    let repo = Arc::new(SqliteMaintenanceRepository::new(db.pool()));
    let enforcer = MaintenanceEnforcer::new(repo.clone(), Arc::new(NoopAuditService));
    let caller = admin_user();
    let w = window(-60, 300, DestructiveActionClass::PackageInstall);
    enforcer.schedule(&caller, w.clone()).await.expect("schedule");
    enforcer.cancel(&caller, w.id).await.expect("cancel");
    let list = repo.list_windows().await.expect("list");
    assert!(list.is_empty());
}