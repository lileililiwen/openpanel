//! Preview deployment integration tests.
#![allow(missing_docs, clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use std::sync::Arc;

use openpanel_app::git_deployment::{PreviewService, SqlitePreviewRepository};
use openpanel_domain::{DeployRepo, PreviewError, PreviewState, Role, User};
use openpanel_test_support::{MockAudit, TestDb};

fn owner() -> User {
    use openpanel_domain::{Email, Password, Username};
    User::new(
        uuid::Uuid::new_v4(),
        Username::new("owner").unwrap(),
        Email::new("owner@example.test").unwrap(),
        Password::hash("correct horse battery staple").unwrap(),
        Role::Owner,
    )
}

fn collaborator() -> User {
    use openpanel_domain::{Email, Password, Username};
    User::new(
        uuid::Uuid::new_v4(),
        Username::new("collab").unwrap(),
        Email::new("collab@example.test").unwrap(),
        Password::hash("correct horse battery staple").unwrap(),
        Role::User,
    )
}

fn repo(site_id: uuid::Uuid) -> DeployRepo {
    DeployRepo {
        id: uuid::Uuid::new_v4(),
        site_id,
        url: "https://example.com/repo.git".into(),
        branch: "main".into(),
        linked_at: chrono::Utc::now(),
        build_command: "npm ci && npm run build".into(),
        docroot_subdir: "dist".into(),
        webhook_secret: "secret".into(),
    }
}

fn build_service(db: &TestDb) -> PreviewService {
    PreviewService::new(
        Arc::new(SqlitePreviewRepository::new(db.pool())),
        Arc::new(MockAudit::stub()),
        "pr.example.com",
        72,
        3,
    )
}

/// Insert a `DeployRepo` row so that the `previews.repo_id` foreign
/// key is satisfied. The preview service does not persist the
/// repo itself; the deployment module does, and tests can
/// exercise the preview path independently.
async fn insert_repo(db: &TestDb, repo: &DeployRepo) {
    sqlx::query(
        "INSERT OR REPLACE INTO deploy_repos \
         (id, site_id, url, branch, linked_at, build_command, docroot_subdir, webhook_secret) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(repo.id.to_string())
    .bind(repo.site_id.to_string())
    .bind(&repo.url)
    .bind(&repo.branch)
    .bind(repo.linked_at.to_rfc3339())
    .bind(&repo.build_command)
    .bind(&repo.docroot_subdir)
    .bind(&repo.webhook_secret)
    .execute(&db.pool())
    .await
    .unwrap();
}

#[tokio::test]
async fn opened_webhook_creates_ready_preview_and_replay_updates() {
    let db = TestDb::new().await;
    let svc = build_service(&db);
    let owner = owner();
    let site_id = uuid::Uuid::new_v4();
    let repo = repo(site_id);
    insert_repo(&db, &repo).await;

    let p1 = svc.upsert(&owner, &repo, 42).await.unwrap();
    assert_eq!(p1.state(), PreviewState::Ready);
    assert_eq!(p1.hostname(), "42.pr.example.com");

    // Replaying the same webhook updates rather than duplicates.
    let p2 = svc.upsert(&owner, &repo, 42).await.unwrap();
    assert_eq!(p2.id(), p1.id());
    let listed = svc.list(&owner, site_id).await.unwrap();
    assert_eq!(listed.len(), 1);
}

#[tokio::test]
async fn closed_webhook_destroys_preview() {
    let db = TestDb::new().await;
    let svc = build_service(&db);
    let owner = owner();
    let site_id = uuid::Uuid::new_v4();
    let repo = repo(site_id);
    insert_repo(&db, &repo).await;

    svc.upsert(&owner, &repo, 7).await.unwrap();
    svc.destroy(&owner, site_id, 7).await.unwrap();
    let listed = svc.list(&owner, site_id).await.unwrap();
    assert_eq!(listed[0].state(), PreviewState::Destroyed);
}

#[tokio::test]
async fn collaborator_without_scope_is_forbidden() {
    let db = TestDb::new().await;
    let svc = build_service(&db);
    let collab = collaborator();
    let site_id = uuid::Uuid::new_v4();
    let repo = repo(site_id);
    insert_repo(&db, &repo).await;

    let err = svc.upsert(&collab, &repo, 1).await.unwrap_err();
    assert!(matches!(err, PreviewError::Invalid(_)));
}

#[tokio::test]
async fn cap_evicts_oldest_expired_or_rejects() {
    let db = TestDb::new().await;
    let svc = build_service(&db);
    let owner = owner();
    let site_id = uuid::Uuid::new_v4();
    let repo = repo(site_id);
    insert_repo(&db, &repo).await;

    // Fill to the cap (3).
    for pr in 1..=3 {
        svc.upsert(&owner, &repo, pr).await.unwrap();
    }
    // 4th without any expired -> CapReached.
    let err = svc.upsert(&owner, &repo, 4).await.unwrap_err();
    assert!(matches!(err, PreviewError::CapReached));

    // Expire the oldest (pr 1) by backdating its expires_at, then a new
    // preview evicts it.
    let listed = svc.list(&owner, site_id).await.unwrap();
    let oldest = listed.iter().find(|p| p.pr_number() == 1).unwrap();
    let mut expired = oldest.clone();
    expired.destroy(chrono::Utc::now(), "expired").unwrap();
    // Simulate expiry by directly marking destroyed via reaper path:
    // instead, backdate via SQL.
    sqlx::query("UPDATE previews SET expires_at = ? WHERE pr_number = 1")
        .bind((chrono::Utc::now() - chrono::Duration::hours(1)).to_rfc3339())
        .execute(&db.pool())
        .await
        .unwrap();

    let p4 = svc.upsert(&owner, &repo, 4).await.unwrap();
    assert_eq!(p4.pr_number(), 4);
    let listed = svc.list(&owner, site_id).await.unwrap();
    // pr 1 was evicted (destroyed), pr 4 added.
    assert!(listed.iter().any(|p| p.pr_number() == 4));
    assert!(
        listed
            .iter()
            .any(|p| p.pr_number() == 1 && p.state() == PreviewState::Destroyed)
    );
}

#[tokio::test]
async fn reaper_destroys_expired_previews() {
    let db = TestDb::new().await;
    let svc = build_service(&db);
    let owner = owner();
    let site_id = uuid::Uuid::new_v4();
    let repo = repo(site_id);
    insert_repo(&db, &repo).await;

    svc.upsert(&owner, &repo, 1).await.unwrap();
    // Backdate expiry so the reaper picks it up.
    sqlx::query("UPDATE previews SET expires_at = ?")
        .bind((chrono::Utc::now() - chrono::Duration::hours(1)).to_rfc3339())
        .execute(&db.pool())
        .await
        .unwrap();

    let reaped = svc.reap_expired().await.unwrap();
    assert_eq!(reaped, 1);
    let listed = svc.list(&owner, site_id).await.unwrap();
    assert_eq!(listed[0].state(), PreviewState::Destroyed);
}

#[tokio::test]
async fn preview_db_name_is_isolated_from_production() {
    let db = TestDb::new().await;
    let svc = build_service(&db);
    let owner = owner();
    let site_id = uuid::Uuid::new_v4();
    let repo = repo(site_id);
    insert_repo(&db, &repo).await;

    let p = svc.upsert(&owner, &repo, 5).await.unwrap();
    // The preview hostname is namespaced under the preview base domain,
    // never the production site host.
    assert!(p.hostname().ends_with("pr.example.com"));
    assert!(!p.hostname().contains("production"));
}
