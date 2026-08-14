//! `container-registry` module composition.

use std::sync::Arc;

use openpanel_core::{AppContext, AuditService, Migration, Module};

use super::{
    repo::{
        SqliteImageRepository, SqliteNamespaceRepository, SqliteScanResultRepository,
    },
    scan::NoopScanHook,
    service::ContainerRegistryService,
    storage::{MemoryStorage, StorageLayer},
};

/// Stable module identifier.
pub const MODULE_NAME: &str = "container-registry";

/// Container registry module: hosted OCI Distribution registry with
/// per-user namespaces, retention policy, and scan hook.
pub struct ContainerRegistryModule {
    service: Arc<ContainerRegistryService>,
    migrations: Vec<Migration>,
}

impl ContainerRegistryModule {
    /// Build the module from the shared `AppContext`. Production
    /// code wires the filesystem-backed storage layer; the test
    /// harness wires an in-memory storage adapter.
    #[allow(clippy::too_many_arguments)]
    pub async fn new(
        ctx: &AppContext,
        audit: Arc<dyn AuditService>,
        config: openpanel_domain::RegistryConfig,
        storage: Option<Arc<dyn StorageLayer>>,
        scan_hook: Option<Arc<dyn super::scan::ScanHook>>,
    ) -> Self {
        let pool = ctx.db.pool().await;
        let namespaces = Arc::new(SqliteNamespaceRepository::new(pool.clone()));
        let images = Arc::new(SqliteImageRepository::new(pool.clone()));
        let scans = Arc::new(SqliteScanResultRepository::new(pool));
        let storage = storage.unwrap_or_else(|| Arc::new(MemoryStorage::new()));
        let scan_hook = scan_hook.unwrap_or_else(|| Arc::new(NoopScanHook));
        let service = Arc::new(ContainerRegistryService::new(
            namespaces,
            images,
            scans,
            storage,
            scan_hook,
            audit,
            config,
        ));
        Self {
            service,
            migrations: vec![Migration {
                module: MODULE_NAME.into(),
                version: "001".into(),
                description: "container registry initial schema".into(),
                sql: crate::migrations::CONTAINER_REGISTRY_V001.into(),
            }],
        }
    }

    /// Access the registry service.
    pub fn service(&self) -> Arc<ContainerRegistryService> {
        self.service.clone()
    }
}

impl Module for ContainerRegistryModule {
    fn name(&self) -> &'static str {
        MODULE_NAME
    }

    fn migrations(&self) -> Vec<Migration> {
        self.migrations.clone()
    }
}