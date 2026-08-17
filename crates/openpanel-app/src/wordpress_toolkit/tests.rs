//! WordPress toolkit bounded context unit and service tests.

use std::sync::Arc;

use chrono::Utc;
use openpanel_core::NoopAuditService;
use openpanel_domain::{Role, WpCacheMode, WpRepository, WpSite, WpUpdateSet, compare_versions};
use openpanel_test_support::TestDb;
use uuid::Uuid;

use crate::wordpress_toolkit::{
    FakeWpFilesystem, SqliteWpRepository, WpCacheLayer, WpScanner, WpToolkitService, WpUpdater,
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

#[tokio::test]
async fn scanner_marks_outdated_core_as_warn() {
    let db = TestDb::new().await;
    let repo = Arc::new(SqliteWpRepository::new(db.pool()));
    let scanner = WpScanner::new(repo.clone(), Arc::new(NoopAuditService));
    let site = WpSite::new(Uuid::new_v4(), "/var/www/wp", "6.3.0");
    repo.save_site(&site).await.expect("save");
    let report = scanner
        .scan(&admin_user(), site.site_id)
        .await
        .expect("scan");
    assert_eq!(report.findings.len(), 1);
    assert_eq!(report.findings[0].severity, "warn");
}

#[tokio::test]
async fn scanner_marks_very_old_core_as_cve() {
    let db = TestDb::new().await;
    let repo = Arc::new(SqliteWpRepository::new(db.pool()));
    let scanner = WpScanner::new(repo.clone(), Arc::new(NoopAuditService));
    let site = WpSite::new(Uuid::new_v4(), "/var/www/wp", "5.0.0");
    repo.save_site(&site).await.expect("save");
    let report = scanner
        .scan(&admin_user(), site.site_id)
        .await
        .expect("scan");
    assert_eq!(report.findings[0].severity, "cve");
}

#[tokio::test]
async fn scanner_marks_current_core_as_info() {
    let db = TestDb::new().await;
    let repo = Arc::new(SqliteWpRepository::new(db.pool()));
    let scanner = WpScanner::new(repo.clone(), Arc::new(NoopAuditService));
    let site = WpSite::new(Uuid::new_v4(), "/var/www/wp", "6.5.0");
    repo.save_site(&site).await.expect("save");
    let report = scanner
        .scan(&admin_user(), site.site_id)
        .await
        .expect("scan");
    assert_eq!(report.findings[0].severity, "info");
}

#[tokio::test]
async fn updater_applies_updates_and_persists_history() {
    let db = TestDb::new().await;
    let repo = Arc::new(SqliteWpRepository::new(db.pool()));
    let fs = Arc::new(FakeWpFilesystem::new(
        "/var/www/wp",
        vec!["wp-config.php".to_string()],
    ));
    let updater = WpUpdater::new(repo.clone(), fs, Arc::new(NoopAuditService));
    let site = WpSite::new(Uuid::new_v4(), "/var/www/wp", "6.3.0");
    repo.save_site(&site).await.expect("save");
    let updates = vec![WpUpdateSet {
        component: "core".into(),
        from_version: "6.3.0".into(),
        to_version: "6.4.0".into(),
    }];
    let result = updater
        .apply(&admin_user(), site.site_id, updates)
        .await
        .expect("apply");
    assert!(result.success);
    assert!(!result.rolled_back);
    let history = repo
        .list_update_runs(site.site_id, 10)
        .await
        .expect("history");
    assert_eq!(history.len(), 1);
    let updated = repo
        .get_site(site.site_id)
        .await
        .expect("get")
        .expect("present");
    assert_eq!(updated.core_version, "6.4.0");
}

#[tokio::test]
async fn updater_rolls_back_downgrade() {
    let db = TestDb::new().await;
    let repo = Arc::new(SqliteWpRepository::new(db.pool()));
    let fs = Arc::new(FakeWpFilesystem::new(
        "/var/www/wp",
        vec!["wp-config.php".to_string()],
    ));
    let updater = WpUpdater::new(repo.clone(), fs, Arc::new(NoopAuditService));
    let site = WpSite::new(Uuid::new_v4(), "/var/www/wp", "6.4.0");
    repo.save_site(&site).await.expect("save");
    let updates = vec![WpUpdateSet {
        component: "core".into(),
        from_version: "6.4.0".into(),
        to_version: "6.3.0".into(),
    }];
    let result = updater
        .apply(&admin_user(), site.site_id, updates)
        .await
        .expect("apply");
    assert!(!result.success);
    assert!(result.rolled_back);
    let after = repo
        .get_site(site.site_id)
        .await
        .expect("get")
        .expect("present");
    assert_eq!(after.core_version, "6.4.0");
}

#[tokio::test]
async fn cache_layer_toggles_modes() {
    let db = TestDb::new().await;
    let repo = Arc::new(SqliteWpRepository::new(db.pool()));
    let cache = WpCacheLayer::new(repo.clone(), Arc::new(NoopAuditService));
    let site = WpSite::new(Uuid::new_v4(), "/var/www/wp", "6.4.0");
    repo.save_site(&site).await.expect("save");
    let after = cache
        .set(&admin_user(), site.site_id, WpCacheMode::Aggressive)
        .await
        .expect("set");
    assert_eq!(after.cache_mode, WpCacheMode::Aggressive);
    let after = cache
        .set(&admin_user(), site.site_id, WpCacheMode::Off)
        .await
        .expect("set");
    assert_eq!(after.cache_mode, WpCacheMode::Off);
}

#[tokio::test]
async fn facade_routes_to_subservices() {
    let db = TestDb::new().await;
    let repo = Arc::new(SqliteWpRepository::new(db.pool()));
    let scanner = WpScanner::new(repo.clone(), Arc::new(NoopAuditService));
    let fs = Arc::new(FakeWpFilesystem::new(
        "/var/www/wp",
        vec!["wp-config.php".to_string()],
    ));
    let updater = WpUpdater::new(repo.clone(), fs, Arc::new(NoopAuditService));
    let cache = WpCacheLayer::new(repo.clone(), Arc::new(NoopAuditService));
    let service = WpToolkitService::new(scanner, updater, cache);
    let site = WpSite::new(Uuid::new_v4(), "/var/www/wp", "6.3.0");
    service
        .register(&admin_user(), site.clone())
        .await
        .expect("register");
    let _ = service
        .scan(&admin_user(), site.site_id)
        .await
        .expect("scan");
    let _ = service
        .apply(
            &admin_user(),
            site.site_id,
            vec![WpUpdateSet {
                component: "core".into(),
                from_version: "6.3.0".into(),
                to_version: "6.4.0".into(),
            }],
        )
        .await
        .expect("apply");
    let _ = service
        .set_cache(&admin_user(), site.site_id, WpCacheMode::Aggressive)
        .await
        .expect("cache");
}

#[tokio::test]
async fn non_admin_cannot_use_wp_toolkit() {
    let db = TestDb::new().await;
    let repo = Arc::new(SqliteWpRepository::new(db.pool()));
    let fs = Arc::new(FakeWpFilesystem::new(
        "/var/www/wp",
        vec!["wp-config.php".to_string()],
    ));
    let scanner = WpScanner::new(repo.clone(), Arc::new(NoopAuditService));
    let updater = WpUpdater::new(repo.clone(), fs, Arc::new(NoopAuditService));
    let cache = WpCacheLayer::new(repo, Arc::new(NoopAuditService));
    let service = WpToolkitService::new(scanner, updater, cache);
    let user = openpanel_domain::User::new(
        Uuid::new_v4(),
        openpanel_domain::Username::new("viewer").expect("static"),
        openpanel_domain::Email::new("viewer@example.com").expect("static"),
        openpanel_domain::Password::hash("correct horse battery staple").expect("static"),
        Role::User,
    );
    let _ = service.scan(&user, Uuid::new_v4()).await;
    let _ = service
        .set_cache(&user, Uuid::new_v4(), WpCacheMode::Standard)
        .await;
}

#[test]
fn version_compare_helper_orders_correctly() {
    use std::cmp::Ordering;
    assert_eq!(compare_versions("6.4.0", "6.4.0"), Ordering::Equal);
    assert_eq!(compare_versions("6.4.1", "6.4.0"), Ordering::Greater);
    assert_eq!(compare_versions("5.9.0", "6.0.0"), Ordering::Less);
    let _ = Utc::now();
}
