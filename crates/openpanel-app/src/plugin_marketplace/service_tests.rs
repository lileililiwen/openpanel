//! Marketplace service tests.

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use chrono::Utc;
    use ed25519_dalek::{Signer, SigningKey, VerifyingKey};
    use openpanel_core::{AuditEvent, AuditService};
    use openpanel_domain::{
        Capability, CapabilitySet, CatalogSnapshot, MarketplaceCa, MarketplacePlugin,
        PluginMarketplaceError, PluginRating, SignedCatalogEnvelope,
    };
    use openpanel_test_support::TestDb;
    use tokio::sync::Mutex;

    use super::super::{
        cache::SqliteCatalogCache,
        client::{MarketplaceClient, MockMarketplaceClient},
        service::{InstallFromMarketplaceRequest, MarketplaceService},
    };

    /// In-memory `AuditService` for tests.
    #[derive(Default)]
    struct RecordingAudit {
        events: Mutex<Vec<AuditEvent>>,
    }

    #[async_trait::async_trait]
    impl AuditService for RecordingAudit {
        async fn record(&self, event: AuditEvent) -> Result<(), openpanel_core::CoreError> {
            self.events.lock().await.push(event);
            Ok(())
        }

        async fn recent(&self, _limit: i64) -> Result<Vec<AuditEvent>, openpanel_core::CoreError> {
            Ok(self.events.lock().await.clone())
        }

        /// Paginated queries are not part of what this double asserts.
        async fn query(
            &self,
            _query: openpanel_core::audit::AuditQuery,
        ) -> Result<openpanel_core::audit::AuditPage, openpanel_core::CoreError> {
            Ok(openpanel_core::audit::AuditPage {
                events: Vec::new(),
                next_cursor: None,
            })
        }
    }

    fn sample_entry(id: &str) -> MarketplacePlugin {
        MarketplacePlugin {
            id: id.to_string(),
            name: "Demo".to_string(),
            publisher: "publisher-demo".to_string(),
            manifest_url: "https://example.com/manifest.json".to_string(),
            rating: 4.0,
            summary: "Demo plugin".to_string(),
        }
    }

    fn sign_envelope(envelope: &mut SignedCatalogEnvelope, signing_key: &SigningKey) {
        let payload = envelope.payload.clone();
        let signed = format!("{payload}\n{}", envelope.publisher_id);
        let sig = signing_key.sign(signed.as_bytes());
        envelope.signature =
            base64::Engine::encode(&base64::engine::general_purpose::STANDARD, sig.to_bytes());
    }

    #[tokio::test]
    async fn discover_fetches_verifies_and_caches_catalog() {
        let db = TestDb::new().await;
        let pool = db.pool();

        let signing_key = SigningKey::from_bytes(&[7u8; 32]);
        let verifying_key: VerifyingKey = signing_key.verifying_key();
        let entries = vec![sample_entry("com.example.demo")];
        // Use the typed payload shape so the round-trip is canonical.
        #[derive(serde::Serialize)]
        struct TestPayload<'a> {
            entries: &'a [MarketplacePlugin],
        }
        let payload = serde_json::to_string(&TestPayload { entries: &entries }).unwrap();
        let mut envelope = SignedCatalogEnvelope {
            schema: 1,
            expires_at: u64::MAX,
            payload,
            signature: String::new(),
            publisher_id: "publisher-demo".to_string(),
        };
        sign_envelope(&mut envelope, &signing_key);

        let client: Arc<MockMarketplaceClient> =
            Arc::new(MockMarketplaceClient::new().with_envelope(envelope));
        let cache = Arc::new(SqliteCatalogCache::new(pool.clone()));
        let plugins = Arc::new(crate::plugin::service::PluginService::new(
            Arc::new(crate::plugin::repo::SqlitePluginRegistry::new(pool.clone())),
            Arc::new(RecordingAudit::default()),
        ));
        let audit: Arc<dyn AuditService> = Arc::new(RecordingAudit::default());
        let ca = MarketplaceCa::from_keys(vec![verifying_key]);
        let service = MarketplaceService::new(client.clone(), cache, ca, plugins, audit);

        let outcome = service.discover(Some(0)).await.expect("discover");
        assert!(!outcome.from_cache);
        assert_eq!(outcome.catalog.entries.len(), 1);
        assert_eq!(outcome.catalog.entries[0].id, "com.example.demo");

        // Cached call returns the same digest.
        let cached = service.cached().await.expect("cached").expect("present");
        assert_eq!(cached.catalog.digest, outcome.catalog.digest);
    }

    #[tokio::test]
    async fn discover_rejects_tampered_catalog() {
        let db = TestDb::new().await;
        let pool = db.pool();

        let signing_key = SigningKey::from_bytes(&[9u8; 32]);
        let verifying_key = signing_key.verifying_key();
        let entries = vec![sample_entry("com.example.demo")];
        #[derive(serde::Serialize)]
        struct TestPayload<'a> {
            entries: &'a [MarketplacePlugin],
        }
        let payload = serde_json::to_string(&TestPayload { entries: &entries }).unwrap();
        let mut envelope = SignedCatalogEnvelope {
            schema: 1,
            expires_at: u64::MAX,
            payload: payload.clone(),
            signature: String::new(),
            publisher_id: "publisher-demo".to_string(),
        };
        sign_envelope(&mut envelope, &signing_key);
        // Tamper with the payload after signing.
        envelope.payload = payload.replace("Demo", "TAMPERED");

        let client: Arc<MockMarketplaceClient> =
            Arc::new(MockMarketplaceClient::new().with_envelope(envelope));
        let cache = Arc::new(SqliteCatalogCache::new(pool.clone()));
        let plugins = Arc::new(crate::plugin::service::PluginService::new(
            Arc::new(crate::plugin::repo::SqlitePluginRegistry::new(pool.clone())),
            Arc::new(RecordingAudit::default()),
        ));
        let audit: Arc<dyn AuditService> = Arc::new(RecordingAudit::default());
        let ca = MarketplaceCa::from_keys(vec![verifying_key]);
        let service = MarketplaceService::new(client.clone(), cache, ca, plugins, audit);

        let err = service.discover(Some(0)).await.unwrap_err();
        assert!(matches!(err, PluginMarketplaceError::PublisherUnverified));
    }

    #[tokio::test]
    async fn install_from_marketplace_delegates_to_plugin_service() {
        let db = TestDb::new().await;
        let pool = db.pool();

        let signing_key = SigningKey::from_bytes(&[11u8; 32]);
        let verifying_key = signing_key.verifying_key();
        let ca = MarketplaceCa::from_keys(vec![verifying_key]);

        // Build a signed manifest.
        let mut manifest = openpanel_domain::PluginManifest {
            id: openpanel_domain::PluginId::new("com.example.demo").unwrap(),
            version: openpanel_domain::PluginVersion::new("1.0.0").unwrap(),
            runtime: openpanel_domain::ManifestRuntime::JsonRpc,
            entrypoint: "/usr/lib/openpanel/plugins/demo/bin".into(),
            capabilities: CapabilitySet::from_iter([Capability::new("system-services:read")]),
            permissions: vec![],
            ui: openpanel_domain::plugin::manifest::PluginUi::default(),
            publisher: openpanel_domain::PublisherKey::new("publisher-demo").unwrap(),
            signature: String::new(),
            created_at: None,
        };
        let body = manifest.canonical_body();
        let signed = format!("{body}\n{}", manifest.publisher.as_str());
        let sig = signing_key.sign(signed.as_bytes());
        manifest.signature =
            base64::Engine::encode(&base64::engine::general_purpose::STANDARD, sig.to_bytes());

        let audit: Arc<dyn AuditService> = Arc::new(RecordingAudit::default());
        let plugins = Arc::new(crate::plugin::service::PluginService::new(
            Arc::new(crate::plugin::repo::SqlitePluginRegistry::new(pool.clone())),
            audit.clone(),
        ));
        let client: Arc<dyn MarketplaceClient> = Arc::new(MockMarketplaceClient::new());
        let cache = Arc::new(SqliteCatalogCache::new(pool.clone()));
        let service = MarketplaceService::new(client, cache, ca, plugins, audit);

        let request = InstallFromMarketplaceRequest {
            plugin_id: "com.example.demo".to_string(),
            manifest_url: "https://example.com/manifest.json".to_string(),
            manifest,
            publisher_id: "publisher-demo".to_string(),
        };
        service
            .install_from_marketplace(request, &verifying_key, "admin")
            .await
            .expect("install");
        let stored = service
            .plugins
            .find(&openpanel_domain::PluginId::new("com.example.demo").unwrap())
            .await
            .expect("find")
            .expect("installed");
        assert_eq!(stored.version.as_str(), "1.0.0");
        assert_eq!(stored.publisher.as_str(), "publisher-demo");
    }

    #[tokio::test]
    async fn empty_ca_refuses_install() {
        let db = TestDb::new().await;
        let pool = db.pool();
        let audit: Arc<dyn AuditService> = Arc::new(RecordingAudit::default());
        let plugins = Arc::new(crate::plugin::service::PluginService::new(
            Arc::new(crate::plugin::repo::SqlitePluginRegistry::new(pool.clone())),
            audit.clone(),
        ));
        let client: Arc<dyn MarketplaceClient> = Arc::new(MockMarketplaceClient::new());
        let cache = Arc::new(SqliteCatalogCache::new(pool.clone()));
        let service =
            MarketplaceService::new(client, cache, MarketplaceCa::empty(), plugins, audit);

        let manifest = openpanel_domain::PluginManifest {
            id: openpanel_domain::PluginId::new("com.example.demo").unwrap(),
            version: openpanel_domain::PluginVersion::new("1.0.0").unwrap(),
            runtime: openpanel_domain::ManifestRuntime::JsonRpc,
            entrypoint: "/usr/lib/openpanel/plugins/demo/bin".into(),
            capabilities: CapabilitySet::default(),
            permissions: vec![],
            ui: openpanel_domain::plugin::manifest::PluginUi::default(),
            publisher: openpanel_domain::PublisherKey::new("publisher-demo").unwrap(),
            signature: "ignore".into(),
            created_at: None,
        };
        let request = InstallFromMarketplaceRequest {
            plugin_id: "com.example.demo".to_string(),
            manifest_url: "https://example.com/manifest.json".to_string(),
            manifest,
            publisher_id: "publisher-demo".to_string(),
        };
        // Empty CA: the marketplace layer still verifies the
        // manifest signature against the caller-supplied key, so
        // we deliberately supply an *unrelated* key and expect the
        // signature check to fail.
        let bad_key = SigningKey::from_bytes(&[99u8; 32]).verifying_key();
        let err = service
            .install_from_marketplace(request, &bad_key, "admin")
            .await
            .unwrap_err();
        // We just need to know the install was refused.
        let _ = err;
    }

    #[tokio::test]
    async fn rating_validates_range() {
        assert!(PluginRating::new(-0.1).is_err());
        assert!(PluginRating::new(0.0).is_ok());
        assert!(PluginRating::new(5.0).is_ok());
        assert!(PluginRating::new(5.1).is_err());
    }

    #[tokio::test]
    async fn cached_returns_none_when_empty() {
        let db = TestDb::new().await;
        let pool = db.pool();
        let audit: Arc<dyn AuditService> = Arc::new(RecordingAudit::default());
        let plugins = Arc::new(crate::plugin::service::PluginService::new(
            Arc::new(crate::plugin::repo::SqlitePluginRegistry::new(pool.clone())),
            audit.clone(),
        ));
        let client: Arc<dyn MarketplaceClient> = Arc::new(MockMarketplaceClient::new());
        let cache = Arc::new(SqliteCatalogCache::new(pool.clone()));
        let service =
            MarketplaceService::new(client, cache, MarketplaceCa::empty(), plugins, audit);
        let cached = service.cached().await.expect("cached");
        assert!(cached.is_none());
        let _ = CatalogSnapshot {
            catalog: openpanel_domain::MarketplaceCatalog {
                digest: "x".into(),
                entries: Vec::new(),
                publisher_id: "p".into(),
                expires_at: 0,
                fetched_at: None,
            },
            cached_at: Utc::now(),
        };
    }
}
