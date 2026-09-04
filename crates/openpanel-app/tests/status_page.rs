//! Status page integration tests: covers enable/disable, slug rotation,
//! publish/unpublish, public view composition, and idempotent regen.
//!
//! These tests use `TestDb` (in-memory SQLite) and the real
//! `SqliteStatusPageRepository` to exercise the service end-to-end
//! without spinning up an HTTP server.

#![allow(missing_docs, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use openpanel_app::synthetic_monitoring::{
    PublicStatusView, SqliteStatusPageRepository, SqliteSyntheticRepository, StatusPageService,
};
use openpanel_core::AuditService;
use openpanel_domain::{
    CheckResult, CheckStatus, CheckType, Email, Password, Role, SyntheticCheck,
    SyntheticRepository, User, Username,
};
use openpanel_test_support::{MockAudit, TestDb};

fn owner() -> User {
    User::new(
        uuid::Uuid::new_v4(),
        Username::new("owner").unwrap(),
        Email::new(format!("owner-{}@example.test", uuid::Uuid::new_v4())).unwrap(),
        Password::hash("correct horse battery staple").unwrap(),
        Role::Owner,
    )
}

fn admin() -> User {
    User::new(
        uuid::Uuid::new_v4(),
        Username::new("admin").unwrap(),
        Email::new(format!("admin-{}@example.test", uuid::Uuid::new_v4())).unwrap(),
        Password::hash("correct horse battery staple").unwrap(),
        Role::Admin,
    )
}

fn regular_user() -> User {
    User::new(
        uuid::Uuid::new_v4(),
        Username::new("user").unwrap(),
        Email::new(format!("user-{}@example.test", uuid::Uuid::new_v4())).unwrap(),
        Password::hash("correct horse battery staple").unwrap(),
        Role::User,
    )
}

fn build_service(
    db: &TestDb,
    audit: Arc<dyn AuditService>,
) -> (StatusPageService, Arc<SqliteSyntheticRepository>) {
    let status_repo = Arc::new(SqliteStatusPageRepository::new(db.pool()));
    let sql_repo = Arc::new(SqliteSyntheticRepository::new(db.pool()));
    let svc = StatusPageService::new(status_repo, sql_repo.clone(), audit);
    (svc, sql_repo)
}

fn new_check(name: &str) -> SyntheticCheck {
    SyntheticCheck {
        id: uuid::Uuid::new_v4(),
        name: name.to_string(),
        kind: CheckType::Http,
        target: "https://example.com/health".into(),
        expected_status: Some(200),
        timeout_secs: 5,
        throttle_secs: 60,
        warn_before_days: 14,
        created_at: chrono::Utc::now(),
        last_run_at: None,
        enabled: true,
    }
}

#[tokio::test]
async fn enable_then_disable_round_trips() {
    let db = TestDb::new().await;
    let audit: Arc<dyn AuditService> = Arc::new(MockAudit::stub());
    let (svc, _) = build_service(&db, audit);

    let page = svc.enable(&owner()).await.unwrap();
    assert!(page.enabled);

    let page = svc.disable(&owner()).await.unwrap();
    assert!(!page.enabled);
}

#[tokio::test]
async fn regenerate_slug_changes_slug_idempotently() {
    let db = TestDb::new().await;
    let audit: Arc<dyn AuditService> = Arc::new(MockAudit::stub());
    let (svc, _) = build_service(&db, audit);
    let _ = svc.enable(&owner()).await.unwrap();

    let original = svc.get().await.unwrap();
    let page = svc.regenerate_slug(&owner()).await.unwrap();
    assert_ne!(page.slug.as_str(), original.slug.as_str());
    assert_eq!(page.slug.as_str().len(), 26);

    let again = svc.regenerate_slug(&owner()).await.unwrap();
    assert_ne!(again.slug.as_str(), page.slug.as_str());
}

#[tokio::test]
async fn publish_creates_then_updates_label() {
    let db = TestDb::new().await;
    let audit: Arc<dyn AuditService> = Arc::new(MockAudit::stub());
    let (svc, synth_repo) = build_service(&db, audit);
    let _ = svc.enable(&owner()).await.unwrap();

    let check = new_check("API");
    synth_repo.save_check(&check).await.unwrap();

    let page = svc.publish(&owner(), check.id, "API health").await.unwrap();
    assert_eq!(page.entries.len(), 1);
    assert_eq!(page.entries[0].label, "API health");

    let page = svc.publish(&owner(), check.id, "API uptime").await.unwrap();
    assert_eq!(page.entries.len(), 1);
    assert_eq!(page.entries[0].label, "API uptime");
}

#[tokio::test]
async fn publish_rejects_empty_label() {
    let db = TestDb::new().await;
    let audit: Arc<dyn AuditService> = Arc::new(MockAudit::stub());
    let (svc, synth_repo) = build_service(&db, audit);
    let check = new_check("API");
    synth_repo.save_check(&check).await.unwrap();

    let err = svc.publish(&owner(), check.id, "").await.unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("empty label"));
}

#[tokio::test]
async fn publish_rejects_unknown_check() {
    let db = TestDb::new().await;
    let audit: Arc<dyn AuditService> = Arc::new(MockAudit::stub());
    let (svc, _) = build_service(&db, audit);

    let err = svc
        .publish(&owner(), uuid::Uuid::new_v4(), "label")
        .await
        .unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("not found") || msg.contains("persistence"));
}

#[tokio::test]
async fn unpublish_removes_entry() {
    let db = TestDb::new().await;
    let audit: Arc<dyn AuditService> = Arc::new(MockAudit::stub());
    let (svc, synth_repo) = build_service(&db, audit);
    let _ = svc.enable(&owner()).await.unwrap();

    let check = new_check("API");
    synth_repo.save_check(&check).await.unwrap();
    let _ = svc.publish(&owner(), check.id, "API").await.unwrap();

    let page = svc.unpublish(&owner(), check.id).await.unwrap();
    assert!(page.entries.is_empty());
}

#[tokio::test]
async fn non_admin_cannot_enable() {
    let db = TestDb::new().await;
    let audit: Arc<dyn AuditService> = Arc::new(MockAudit::stub());
    let (svc, _) = build_service(&db, audit);

    let err = svc.enable(&regular_user()).await.unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("forbidden"));
}

#[tokio::test]
async fn admin_can_enable() {
    let db = TestDb::new().await;
    let audit: Arc<dyn AuditService> = Arc::new(MockAudit::stub());
    let (svc, _) = build_service(&db, audit);

    let page = svc.enable(&admin()).await.unwrap();
    assert!(page.enabled);
}

#[tokio::test]
async fn public_view_returns_disabled_for_disabled_page() {
    let db = TestDb::new().await;
    let audit: Arc<dyn AuditService> = Arc::new(MockAudit::stub());
    let (svc, _) = build_service(&db, audit);

    let err = svc.public_view().await.unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("disabled"));
}

#[tokio::test]
async fn public_view_for_returns_disabled_for_wrong_slug() {
    let db = TestDb::new().await;
    let audit: Arc<dyn AuditService> = Arc::new(MockAudit::stub());
    let (svc, _) = build_service(&db, audit);
    let _ = svc.enable(&owner()).await.unwrap();

    let err = svc.public_view_for("wrong-slug").await.unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("disabled"));
}

#[tokio::test]
async fn public_view_for_returns_view_for_correct_slug() {
    let db = TestDb::new().await;
    let audit: Arc<dyn AuditService> = Arc::new(MockAudit::stub());
    let (svc, synth_repo) = build_service(&db, audit);
    let _ = svc.enable(&owner()).await.unwrap();

    let check = new_check("API");
    synth_repo.save_check(&check).await.unwrap();
    let page = svc.publish(&owner(), check.id, "API").await.unwrap();

    let view: PublicStatusView = svc.public_view_for(page.slug.as_str()).await.unwrap();
    assert_eq!(view.entries.len(), 1);
    assert_eq!(view.entries[0].label, "API");
}

#[tokio::test]
async fn derive_incidents_propagates_through_public_view() {
    let db = TestDb::new().await;
    let audit: Arc<dyn AuditService> = Arc::new(MockAudit::stub());
    let (svc, synth_repo) = build_service(&db, audit);
    let _ = svc.enable(&owner()).await.unwrap();

    let check = new_check("API");
    synth_repo.save_check(&check).await.unwrap();
    let _ = svc.publish(&owner(), check.id, "API").await.unwrap();

    let now = chrono::Utc::now();
    let base = now - chrono::Duration::minutes(10);
    for i in 0..6 {
        let status = if (2..=4).contains(&i) {
            CheckStatus::Fail
        } else {
            CheckStatus::Ok
        };
        let result = CheckResult {
            id: uuid::Uuid::new_v4(),
            check_id: check.id,
            ran_at: base + chrono::Duration::minutes(i),
            latency_ms: 42,
            http_status: Some(200),
            cert_days_remaining: None,
            status,
            message: String::new(),
        };
        synth_repo.save_result(&result).await.unwrap();
    }

    let page = svc.get().await.unwrap();
    let view = svc.public_view_for(page.slug.as_str()).await.unwrap();
    assert!(!view.incidents.is_empty(), "expected at least one incident");
    assert_eq!(view.incidents[0].outcome, CheckStatus::Fail);
}
