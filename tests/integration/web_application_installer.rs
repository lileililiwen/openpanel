//! Web application installer integration tests: plan lifecycle,
//! idempotency replay, installed-record listing, and uninstall.

use openpanel_domain::{InstallArtifact, InstallDb, InstallOverlay, WebApplicationInstallerError};
use uuid::Uuid;

use crate::common::*;

#[tokio::test]
async fn plan_lifecycle_and_content_hash() {
    let server = TestServer::new().await;
    let svc = server.web_application_installer();

    let site_id = Uuid::new_v4();
    let plan = svc
        .plan(
            "admin",
            "wordpress",
            site_id,
            "/var/www/example.com/wordpress",
            vec![InstallArtifact::new(
                "wp.tar.gz",
                "https://example.com/wp.tar.gz",
                "deadbeef",
                1024,
            )],
            InstallDb::Fresh {
                name: "wp".to_string(),
            },
            vec![InstallOverlay {
                path: "wp-config.php".to_string(),
                body: "<?php return [];".to_string(),
            }],
            vec![],
        )
        .await
        .expect("plan");
    assert_eq!(plan.app_id(), "wordpress");
    assert_eq!(plan.site_id(), site_id);
    assert!(!plan.content_hash().is_empty());
    assert!(plan.is_valid_at(chrono::Utc::now()));

    // Re-load from persistence.
    let loaded = svc
        .repo()
        .find_plan(plan.id())
        .await
        .expect("find")
        .expect("present");
    assert_eq!(loaded.content_hash(), plan.content_hash());
    assert_eq!(loaded.install_path(), "/var/www/example.com/wordpress");
}

#[tokio::test]
async fn idempotency_replay_returns_prior_run() {
    let server = TestServer::new().await;
    let svc = server.web_application_installer();

    let site_id = Uuid::new_v4();
    let plan = svc
        .plan(
            "admin",
            "ghost",
            site_id,
            "/var/www/example.com/ghost",
            vec![],
            InstallDb::None,
            vec![],
            vec![],
        )
        .await
        .expect("plan");

    // A run with no artifacts downloads nothing and writes nothing;
    // the idempotency key must map the second call to the first run.
    let run1 = svc
        .run("admin", plan.id(), chrono::Utc::now(), Some("idem-key-1"))
        .await
        .expect("run1");
    let run2 = svc
        .run("admin", plan.id(), chrono::Utc::now(), Some("idem-key-1"))
        .await
        .expect("run2 (replay)");
    assert_eq!(run1.id(), run2.id());
    assert_eq!(run1.app_id(), "ghost");
    assert!(run1.post_install_url().is_some());
}

#[tokio::test]
async fn missing_confirmation_rejects_run() {
    let server = TestServer::new().await;
    let svc = server.web_application_installer();

    let site_id = Uuid::new_v4();
    let plan = svc
        .plan(
            "admin",
            "wordpress",
            site_id,
            "/var/www/example.com/wordpress",
            vec![],
            InstallDb::None,
            vec![],
            vec![],
        )
        .await
        .expect("plan");

    // `confirmed_at` outside the ±60s window => PlanExpired.
    let old = chrono::Utc::now() - chrono::Duration::minutes(5);
    let err = svc
        .run("admin", plan.id(), old, None)
        .await
        .expect_err("expired confirmation");
    assert!(matches!(err, WebApplicationInstallerError::PlanExpired));
}

#[tokio::test]
async fn installed_list_and_uninstall_mark_removed() {
    let server = TestServer::new().await;
    let svc = server.web_application_installer();

    let site_id = Uuid::new_v4();
    let plan = svc
        .plan(
            "admin",
            "wordpress",
            site_id,
            "/var/www/example.com/wordpress",
            vec![],
            InstallDb::None,
            vec![],
            vec![],
        )
        .await
        .expect("plan");
    let run = svc
        .run("admin", plan.id(), chrono::Utc::now(), None)
        .await
        .expect("run");
    let rows = svc.list_installed(site_id).await.expect("list");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].install_id(), run.install_id());

    svc.uninstall("admin", run.install_id(), false)
        .await
        .expect("uninstall");
    let rows = svc.list_installed(site_id).await.expect("list2");
    assert_eq!(rows.len(), 1);
    assert!(rows[0].removed_at().is_some());
}
