//! Site cache and CDN integration tests: policy persistence, CDN
//! integration lifecycle, and purge orchestration through the real
//! SQLite-backed service and adapter registry.

use openpanel_domain::{CacheLevel, CdnKind, CdnZone, PurgeRequest, SiteCacheCdnError};
use uuid::Uuid;

use crate::common::*;

#[tokio::test]
async fn cache_policy_persists_and_round_trips() {
    let server = TestServer::new().await;
    let service = server.site_cache_cdn();

    let site_id = Uuid::new_v4();
    let policy = service.cache_policy(site_id).await.expect("default");
    assert_eq!(policy.ttl_seconds(), 60);
    assert!(!policy.bypasses("/index.html"));

    let updated = openpanel_domain::SiteCachePolicy::with_ttl(site_id, 300)
        .unwrap()
        .with_bypass_paths(vec!["/wp-admin/*".to_string()])
        .unwrap();
    service
        .set_cache_policy("admin", &updated)
        .await
        .expect("persist");

    let loaded = service.cache_policy(site_id).await.expect("reload");
    assert_eq!(loaded.ttl_seconds(), 300);
    assert!(loaded.bypasses("/wp-admin/index.php"));
    assert!(!loaded.bypasses("/index.html"));
}

#[tokio::test]
async fn cdn_integration_lifecycle_and_purge() {
    let server = TestServer::new().await;
    let service = server.site_cache_cdn();

    let integration = service
        .create_integration("admin", "edge", CdnKind::GenericHttp, "opaque-cfg")
        .await
        .expect("create");
    assert_eq!(integration.kind(), CdnKind::GenericHttp);

    let listed = service.list_integrations().await.expect("list");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].kind(), CdnKind::GenericHttp);

    // Purge through the generic-HTTP adapter registered by the module.
    let zone = CdnZone::new("z1", "example.com");
    let request = PurgeRequest::new(integration.id(), vec!["/a".to_string(), "/b".to_string()])
        .expect("purge request");
    let registry = service.registry();
    let adapter = registry.get(CdnKind::GenericHttp).expect("adapter");
    let receipt = service
        .purge("admin", adapter, &zone, &request)
        .await
        .expect("purge");
    assert_eq!(receipt.purged(), &["/a".to_string(), "/b".to_string()]);

    let recent = service
        .recent_purges(integration.id())
        .await
        .expect("recent");
    assert_eq!(recent.len(), 1);
    assert_eq!(recent[0].purged(), &["/a".to_string(), "/b".to_string()]);

    // The registry rejects unregistered provider kinds.
    let err = match service.registry().get(CdnKind::Cloudflare) {
        Err(e) => e,
        Ok(_) => panic!("must reject unregistered kind"),
    };
    assert!(matches!(err, SiteCacheCdnError::Adapter(_)));

    service
        .delete_integration("admin", integration.id())
        .await
        .expect("delete");
    assert!(service.list_integrations().await.expect("empty").is_empty());
}

#[tokio::test]
async fn purge_request_rejects_more_than_limit() {
    let paths = (0..=100).map(|i| format!("/p/{i}")).collect::<Vec<_>>();
    let err = PurgeRequest::new(Uuid::new_v4(), paths).expect_err("too many");
    assert!(matches!(err, SiteCacheCdnError::TooManyPurgePaths(101)));
}

#[tokio::test]
async fn cache_level_wire_names() {
    let mut names = Vec::new();
    for level in [
        CacheLevel::Off,
        CacheLevel::Basic,
        CacheLevel::Standard,
        CacheLevel::Aggressive,
    ] {
        names.push(level.as_str());
    }
    assert_eq!(names, vec!["off", "basic", "standard", "aggressive"]);
}
