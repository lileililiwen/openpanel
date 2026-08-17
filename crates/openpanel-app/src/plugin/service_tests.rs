//! Plugin service tests.

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use openpanel_core::{AuditAction, AuditOutcome, AuditService, audit::AuditEvent};
    use openpanel_domain::plugin::{
        Capability, CapabilitySet, ManifestRuntime, PluginId, PluginManifest, PluginVersion,
        PublisherKey, manifest::PluginUi,
    };
    use openpanel_test_support::TestDb;
    use tokio::sync::Mutex;

    use super::super::service::PluginService;

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
    }

    fn sample_manifest() -> PluginManifest {
        PluginManifest {
            id: PluginId::new("com.example.demo").unwrap(),
            version: PluginVersion::new("1.2.3").unwrap(),
            runtime: ManifestRuntime::JsonRpc,
            entrypoint: "/usr/lib/openpanel/plugins/demo/bin".into(),
            capabilities: CapabilitySet::from_iter([Capability::new("system-services:read")]),
            permissions: vec![],
            ui: PluginUi::default(),
            publisher: PublisherKey::new("publisher-demo").unwrap(),
            signature: "sig".into(),
            created_at: None,
        }
    }

    #[tokio::test]
    async fn install_manifest_persists_record() {
        let db = TestDb::new().await;
        let pool = db.pool();
        let audit: Arc<dyn AuditService> = Arc::new(RecordingAudit::default());
        let service = PluginService::new(
            Arc::new(crate::plugin::repo::SqlitePluginRegistry::new(pool.clone())),
            audit.clone(),
        );
        let manifest = sample_manifest();
        service
            .install_manifest(&manifest, "admin")
            .await
            .expect("install");
        let record = service
            .find(&manifest.id)
            .await
            .expect("find")
            .expect("present");
        assert_eq!(record.version.as_str(), "1.2.3");
        assert_eq!(record.publisher.as_str(), "publisher-demo");
    }

    #[tokio::test]
    async fn install_twice_refuses_duplicate() {
        let db = TestDb::new().await;
        let pool = db.pool();
        let audit: Arc<dyn AuditService> = Arc::new(RecordingAudit::default());
        let service = PluginService::new(
            Arc::new(crate::plugin::repo::SqlitePluginRegistry::new(pool.clone())),
            audit,
        );
        let manifest = sample_manifest();
        service
            .install_manifest(&manifest, "admin")
            .await
            .expect("install");
        let err = service
            .install_manifest(&manifest, "admin")
            .await
            .unwrap_err();
        let _ = err;
    }

    #[tokio::test]
    async fn enable_and_disable_transitions() {
        let db = TestDb::new().await;
        let pool = db.pool();
        let audit: Arc<dyn AuditService> = Arc::new(RecordingAudit::default());
        let service = PluginService::new(
            Arc::new(crate::plugin::repo::SqlitePluginRegistry::new(pool.clone())),
            audit,
        );
        let manifest = sample_manifest();
        service
            .install_manifest(&manifest, "admin")
            .await
            .expect("install");
        service.enable(&manifest.id, "admin").await.expect("enable");
        let after_enable = service.find(&manifest.id).await.unwrap().unwrap();
        assert_eq!(after_enable.status, openpanel_domain::PluginStatus::Enabled);
        service
            .disable(&manifest.id, "admin")
            .await
            .expect("disable");
        let after_disable = service.find(&manifest.id).await.unwrap().unwrap();
        assert_eq!(
            after_disable.status,
            openpanel_domain::PluginStatus::Disabled
        );
    }

    #[tokio::test]
    async fn uninstall_removes_record() {
        let db = TestDb::new().await;
        let pool = db.pool();
        let audit: Arc<dyn AuditService> = Arc::new(RecordingAudit::default());
        let service = PluginService::new(
            Arc::new(crate::plugin::repo::SqlitePluginRegistry::new(pool.clone())),
            audit,
        );
        let manifest = sample_manifest();
        service
            .install_manifest(&manifest, "admin")
            .await
            .expect("install");
        service
            .uninstall(&manifest.id, "admin")
            .await
            .expect("uninstall");
        let after = service.find(&manifest.id).await.unwrap();
        assert!(after.is_none());
    }
}
