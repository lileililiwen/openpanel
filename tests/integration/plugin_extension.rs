//! Plugin extension framework integration tests: signed manifest
//! install, lifecycle, and registry listing.

use openpanel_domain::{
    CapabilitySet, ManifestRuntime, PluginId, PluginStatus, PluginVersion, PublisherKey,
};
use uuid::Uuid;

use crate::common::*;

fn sample_manifest() -> openpanel_domain::PluginManifest {
    openpanel_domain::PluginManifest {
        id: PluginId::new(format!("com.example.demo-{}", Uuid::new_v4().simple())).unwrap(),
        version: PluginVersion::new("1.2.3").unwrap(),
        runtime: ManifestRuntime::JsonRpc,
        entrypoint: "/usr/lib/openpanel/plugins/demo/bin".into(),
        capabilities: CapabilitySet::from_names(["system-services:read"]),
        permissions: vec![],
        ui: Default::default(),
        publisher: PublisherKey::new("publisher-demo").unwrap(),
        signature: String::new(),
        created_at: None,
    }
}

#[tokio::test]
async fn plugin_install_enable_disable_list() {
    let server = TestServer::new().await;
    let svc = server.plugins();

    let manifest = sample_manifest();
    svc.install_manifest(&manifest, "admin")
        .await
        .expect("install");

    let rows = svc.list().await.expect("list");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, manifest.id);
    assert_eq!(rows[0].status, PluginStatus::Installed);

    svc.enable(&manifest.id, "admin").await.expect("enable");
    let rows = svc.list().await.expect("list2");
    assert_eq!(rows[0].status, PluginStatus::Enabled);

    svc.disable(&manifest.id, "admin").await.expect("disable");
    let rows = svc.list().await.expect("list3");
    assert_eq!(rows[0].status, PluginStatus::Disabled);

    svc.uninstall(&manifest.id, "admin")
        .await
        .expect("uninstall");
    assert!(svc.list().await.expect("empty").is_empty());
}

#[tokio::test]
async fn duplicate_install_is_refused() {
    let server = TestServer::new().await;
    let svc = server.plugins();

    let manifest = sample_manifest();
    svc.install_manifest(&manifest, "admin")
        .await
        .expect("first install");
    let err = svc
        .install_manifest(&manifest, "admin")
        .await
        .expect_err("duplicate must be refused");
    assert!(
        format!("{err}").contains("already installed"),
        "unexpected: {err}"
    );
}

#[tokio::test]
async fn manifest_id_and_version_validate() {
    assert!(PluginId::new("com.example.demo").is_ok());
    assert!(PluginId::new("Bad_ID").is_err());
    assert!(PluginVersion::new("1.2.3").is_ok());
    assert!(PluginVersion::new("1.2").is_err());
}
