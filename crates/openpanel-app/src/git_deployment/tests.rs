//! Git deployment bounded context unit and service tests.

use std::sync::Arc;

use chrono::Utc;
use openpanel_core::NoopAuditService;
use openpanel_domain::{DeployRepo, DeployRepository, Role, verify_webhook};
use openpanel_test_support::TestDb;
use uuid::Uuid;

use crate::git_deployment::{
    DeployService, SqliteDeployRepository, WebhookVerifier,
};

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

fn make_repo(site_id: Uuid) -> DeployRepo {
    DeployRepo {
        id: Uuid::new_v4(),
        site_id,
        url: "https://example.com/repo.git".into(),
        branch: "main".into(),
        linked_at: Utc::now(),
        build_command: "echo build".into(),
        docroot_subdir: "app/public".into(),
        webhook_secret: "this-is-a-32-byte-secret!".into(),
    }
}

#[tokio::test]
async fn link_repo_persists_and_validates() {
    let db = TestDb::new().await;
    let repo = Arc::new(SqliteDeployRepository::new(db.pool()));
    let service = DeployService::new(repo.clone(), Arc::new(NoopAuditService));
    let caller = admin_user();
    let site_id = Uuid::new_v4();
    let r = make_repo(site_id);
    service.link(&caller, r.clone()).await.expect("link");
    let loaded = repo.get_repo(site_id).await.expect("get").expect("present");
    assert_eq!(loaded.url, r.url);
}

#[tokio::test]
async fn link_repo_rejects_invalid_url() {
    let db = TestDb::new().await;
    let repo = Arc::new(SqliteDeployRepository::new(db.pool()));
    let service = DeployService::new(repo, Arc::new(NoopAuditService));
    let caller = admin_user();
    let mut r = make_repo(Uuid::new_v4());
    r.url = "http://insecure.example.com/repo.git".into();
    let res = service.link(&caller, r).await;
    assert!(matches!(
        res,
        Err(openpanel_domain::DeployError::InvalidRepoUrl(_))
    ));
}

#[tokio::test]
async fn link_repo_rejects_chroot_escape() {
    let db = TestDb::new().await;
    let repo = Arc::new(SqliteDeployRepository::new(db.pool()));
    let service = DeployService::new(repo, Arc::new(NoopAuditService));
    let caller = admin_user();
    let mut r = make_repo(Uuid::new_v4());
    r.docroot_subdir = "../etc".into();
    let res = service.link(&caller, r).await;
    assert!(matches!(
        res,
        Err(openpanel_domain::DeployError::OutsideChroot(_))
    ));
}

#[tokio::test]
async fn deploy_runs_and_persists_history() {
    let db = TestDb::new().await;
    let repo = Arc::new(SqliteDeployRepository::new(db.pool()));
    let service = DeployService::new(repo.clone(), Arc::new(NoopAuditService));
    let caller = admin_user();
    let site_id = Uuid::new_v4();
    service
        .link(&caller, make_repo(site_id))
        .await
        .expect("link");
    let run = service
        .deploy(&caller, site_id, "abc123")
        .await
        .expect("deploy");
    assert_eq!(run.commit_sha, "abc123");
    let history = service.list_runs(site_id).await.expect("history");
    assert_eq!(history.len(), 1);
}

#[tokio::test]
async fn unlink_removes_repo() {
    let db = TestDb::new().await;
    let repo = Arc::new(SqliteDeployRepository::new(db.pool()));
    let service = DeployService::new(repo.clone(), Arc::new(NoopAuditService));
    let caller = admin_user();
    let site_id = Uuid::new_v4();
    service
        .link(&caller, make_repo(site_id))
        .await
        .expect("link");
    service.unlink(&caller, site_id).await.expect("unlink");
    let loaded = repo.get_repo(site_id).await.expect("get");
    assert!(loaded.is_none());
}

#[tokio::test]
async fn non_admin_cannot_link() {
    let db = TestDb::new().await;
    let repo = Arc::new(SqliteDeployRepository::new(db.pool()));
    let service = DeployService::new(repo, Arc::new(NoopAuditService));
    let user = non_admin_user();
    let res = service.link(&user, make_repo(Uuid::new_v4())).await;
    assert!(matches!(
        res,
        Err(openpanel_domain::DeployError::Forbidden)
    ));
}

#[tokio::test]
async fn webhook_verifier_accepts_matching_signature() {
    let db = TestDb::new().await;
    let repo = Arc::new(SqliteDeployRepository::new(db.pool()));
    let service = DeployService::new(repo.clone(), Arc::new(NoopAuditService));
    let verifier = WebhookVerifier::new();
    let caller = admin_user();
    let site_id = Uuid::new_v4();
    let r = make_repo(site_id);
    service.link(&caller, r.clone()).await.expect("link");
    let body = b"{\"ref\":\"main\"}";
    let sig = format!(
        "{:016x}",
        {
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};
            let mut hasher = DefaultHasher::new();
            r.webhook_secret.hash(&mut hasher);
            body.hash(&mut hasher);
            hasher.finish()
        }
    );
    assert!(verifier.verify(repo, site_id, body, Some(&sig)).await.is_ok());
}

#[tokio::test]
async fn webhook_verifier_rejects_bad_signature() {
    let db = TestDb::new().await;
    let repo = Arc::new(SqliteDeployRepository::new(db.pool()));
    let service = DeployService::new(repo.clone(), Arc::new(NoopAuditService));
    let verifier = WebhookVerifier::new();
    let caller = admin_user();
    let site_id = Uuid::new_v4();
    service.link(&caller, make_repo(site_id)).await.expect("link");
    let body = b"{\"ref\":\"main\"}";
    let res = verifier.verify(repo, site_id, body, Some("wrong")).await;
    assert!(matches!(
        res,
        Err(openpanel_domain::DeployError::InvalidWebhookSignature)
    ));
}

#[test]
fn webhook_helper_accepts_matching() {
    let body = b"{}";
    let secret = "0123456789abcdef";
    let sig = format!(
        "{:016x}",
        {
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};
            let mut hasher = DefaultHasher::new();
            secret.hash(&mut hasher);
            body.hash(&mut hasher);
            hasher.finish()
        }
    );
    assert!(verify_webhook(secret, body, Some(&sig)).is_ok());
}

#[tokio::test]
async fn webhook_verifier_rejects_missing_signature() {
    let db = TestDb::new().await;
    let repo = Arc::new(SqliteDeployRepository::new(db.pool()));
    let service = DeployService::new(repo.clone(), Arc::new(NoopAuditService));
    let verifier = WebhookVerifier::new();
    let caller = admin_user();
    let site_id = Uuid::new_v4();
    service.link(&caller, make_repo(site_id)).await.expect("link");
    let res = verifier.verify(repo, site_id, b"{}".as_ref(), None).await;
    assert!(matches!(
        res,
        Err(openpanel_domain::DeployError::InvalidWebhookSignature)
    ));
}