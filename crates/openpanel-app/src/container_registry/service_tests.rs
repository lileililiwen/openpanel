//! Container registry service tests.

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use chrono::Utc;
    use openpanel_core::{AuditService, audit::AuditEvent};
    use openpanel_domain::container_registry::{
        ImageDigest, NamespaceId, RegistryConfig, image::ScanStatus, retention::RetentionPolicy,
    };
    use openpanel_test_support::TestDb;
    use tokio::sync::Mutex;
    use uuid::Uuid;

    use super::super::{
        repo::{
            ImageRepository, NamespaceRepository, ScanResultRepository, SqliteImageRepository,
            SqliteNamespaceRepository, SqliteScanResultRepository,
        },
        scan::NoopScanHook,
        service::{ContainerRegistryService, ImageBlob, PushRequest},
        storage::{MemoryStorage, StorageLayer},
    };

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

    fn sample_digest(hex: u8) -> String {
        let body: String = std::iter::repeat_n(hex, 64)
            .map(|b| {
                if b < 10 {
                    (b'0' + b) as char
                } else {
                    (b'a' + (b - 10)) as char
                }
            })
            .collect();
        format!("sha256:{body}")
    }

    async fn make_service(
        scan_on_push: bool,
    ) -> (
        Arc<ContainerRegistryService>,
        Arc<RecordingAudit>,
        Arc<dyn StorageLayer>,
    ) {
        let db = TestDb::new().await;
        let pool = db.pool();
        let namespaces: Arc<dyn NamespaceRepository> =
            Arc::new(SqliteNamespaceRepository::new(pool.clone()));
        let images: Arc<dyn ImageRepository> = Arc::new(SqliteImageRepository::new(pool.clone()));
        let scans: Arc<dyn ScanResultRepository> = Arc::new(SqliteScanResultRepository::new(pool));
        let storage: Arc<dyn StorageLayer> = Arc::new(MemoryStorage::new());
        let audit: Arc<RecordingAudit> = Arc::new(RecordingAudit::default());
        let config = RegistryConfig {
            storage_root: std::path::PathBuf::from("/var/lib/openpanel/registry"),
            retention: RetentionPolicy {
                max_images_per_ns: Some(2),
                max_age_days: None,
            },
            scan_on_push,
        };
        let service = Arc::new(ContainerRegistryService::new(
            namespaces,
            images,
            scans,
            storage.clone(),
            Arc::new(NoopScanHook),
            audit.clone(),
            config,
        ));
        (service, audit, storage)
    }

    #[tokio::test]
    async fn push_stores_image_and_records_audit() {
        let (service, audit, storage) = make_service(false).await;
        let owner = Uuid::new_v4();
        let ns = service
            .create_namespace(NamespaceId::new("alpha").unwrap(), owner, 10_000)
            .await
            .expect("ns");
        let req = PushRequest {
            namespace: ns.namespace_id.clone(),
            manifest_digest: ImageDigest::new(sample_digest(1)).unwrap(),
            reference: Some("v1".into()),
            manifest_bytes: b"{}".to_vec(),
            blobs: vec![ImageBlob {
                digest: ImageDigest::new(sample_digest(2)).unwrap(),
                bytes: vec![0u8; 64],
            }],
        };
        let result = service.push(owner, req, "owner").await.expect("push");
        assert_eq!(result.image.size_bytes, 64 + 2);
        // Storage has both manifest and blob.
        let manifest_path = format!("alpha/manifests/{}", result.image.digest.as_str());
        let blob_path = format!("alpha/blobs/{}", sample_digest(2));
        assert!(storage.read(&manifest_path).await.unwrap().is_some());
        assert!(storage.read(&blob_path).await.unwrap().is_some());
        let recorded = audit.events.lock().await.clone();
        assert!(
            recorded
                .iter()
                .any(|e| matches!(e.action, openpanel_core::AuditAction::RegistryImagePushed))
        );
    }

    #[tokio::test]
    async fn push_rejects_quota_overflow() {
        let (service, _audit, _storage) = make_service(false).await;
        let owner = Uuid::new_v4();
        let ns = service
            .create_namespace(NamespaceId::new("alpha").unwrap(), owner, 100)
            .await
            .expect("ns");
        let req = PushRequest {
            namespace: ns.namespace_id.clone(),
            manifest_digest: ImageDigest::new(sample_digest(1)).unwrap(),
            reference: None,
            manifest_bytes: vec![0u8; 200],
            blobs: vec![],
        };
        let err = service.push(owner, req, "owner").await.unwrap_err();
        let _ = err;
    }

    #[tokio::test]
    async fn push_rejects_cross_namespace_attempt() {
        let (service, _audit, _storage) = make_service(false).await;
        let owner_a = Uuid::new_v4();
        let owner_b = Uuid::new_v4();
        let ns_a = service
            .create_namespace(NamespaceId::new("alpha").unwrap(), owner_a, 1024)
            .await
            .expect("ns a");
        let req = PushRequest {
            namespace: ns_a.namespace_id.clone(),
            manifest_digest: ImageDigest::new(sample_digest(3)).unwrap(),
            reference: None,
            manifest_bytes: b"{}".to_vec(),
            blobs: vec![],
        };
        let err = service.push(owner_b, req, "attacker").await.unwrap_err();
        let _ = err;
    }

    #[tokio::test]
    async fn push_with_scan_records_clean_status() {
        let (service, _audit, _storage) = make_service(true).await;
        let owner = Uuid::new_v4();
        let ns = service
            .create_namespace(NamespaceId::new("alpha").unwrap(), owner, 10_000)
            .await
            .expect("ns");
        let req = PushRequest {
            namespace: ns.namespace_id.clone(),
            manifest_digest: ImageDigest::new(sample_digest(4)).unwrap(),
            reference: None,
            manifest_bytes: b"{}".to_vec(),
            blobs: vec![],
        };
        let result = service.push(owner, req, "owner").await.expect("push");
        assert_eq!(result.image.scan_status, ScanStatus::Clean);
        assert!(result.scan.is_some());
    }

    #[tokio::test]
    async fn retention_prunes_over_count() {
        let (service, _audit, _storage) = make_service(false).await;
        let owner = Uuid::new_v4();
        let ns = service
            .create_namespace(NamespaceId::new("alpha").unwrap(), owner, 10_000)
            .await
            .expect("ns");
        for hex in 1u8..=3 {
            let req = PushRequest {
                namespace: ns.namespace_id.clone(),
                manifest_digest: ImageDigest::new(sample_digest(hex)).unwrap(),
                reference: None,
                manifest_bytes: b"{}".to_vec(),
                blobs: vec![],
            };
            service.push(owner, req, "owner").await.expect("push");
            // Force `pushed_at` ordering: each subsequent push is newer.
            tokio::time::sleep(std::time::Duration::from_millis(2)).await;
        }
        let images = service.list_images(&ns.namespace_id).await.expect("list");
        // The first image pushed should have been pruned (count=2).
        assert!(images.iter().all(|i| i.digest.as_str() != sample_digest(1)));
        assert_eq!(images.len(), 2);
        let _ = Utc::now();
    }
}
