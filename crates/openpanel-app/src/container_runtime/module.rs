//! Container runtime module composition.

use std::sync::Arc;

use openpanel_core::{AppContext, AuditService, Migration, Module};
use openpanel_domain::PlanQuotaCaps;

use super::{
    metrics_poller::MetricsPollerTask,
    registry_adapter::{NoopRegistryHostAdapter, RegistryHostAdapter},
    repo::SqliteContainerRuntimeRepository,
    service::ContainerRuntimeService,
};

/// Stable module identifier.
pub const MODULE_NAME: &str = "container-runtime";

/// Container runtime module: per-user quota, registry
/// credentials, metrics, and network egress accounting.
pub struct ContainerRuntimeModule {
    service: Arc<ContainerRuntimeService>,
    migrations: Vec<Migration>,
    repo: Arc<SqliteContainerRuntimeRepository>,
}

impl ContainerRuntimeModule {
    /// Build the module from the shared `AppContext`.
    ///
    /// `master_key` is the already-decoded 32-byte AES master
    /// key (the same one passed to `db_crypto::decode_master_key`
    /// by other bounded contexts).
    /// The registry adapter is injectable: production wires a
    /// real one; tests and offline CLI builds wire
    /// `NoopRegistryHostAdapter`.
    #[allow(clippy::too_many_arguments)]
    pub async fn new(
        ctx: &AppContext,
        audit: Arc<dyn AuditService>,
        master_key: [u8; super::crypto::KEY_LEN],
        plan_caps: PlanQuotaCaps,
        registry_adapter: Option<Arc<dyn RegistryHostAdapter>>,
    ) -> Self {
        let pool = ctx.db.pool().await;
        let repo = Arc::new(SqliteContainerRuntimeRepository::new(pool));
        let registry = registry_adapter
            .unwrap_or_else(|| Arc::new(NoopRegistryHostAdapter) as Arc<dyn RegistryHostAdapter>);
        let service = Arc::new(ContainerRuntimeService::from_master_key(
            repo.clone(),
            audit,
            registry,
            master_key,
            plan_caps,
        ));
        Self {
            service,
            migrations: vec![Migration {
                module: MODULE_NAME.into(),
                version: "001".into(),
                description: "container runtime initial schema".into(),
                sql: crate::migrations::CONTAINER_RUNTIME_V001.into(),
            }],
            repo,
        }
    }

    /// Access the runtime service.
    pub fn service(&self) -> Arc<ContainerRuntimeService> {
        self.service.clone()
    }

    /// Build a metrics retention poller task bound to the same
    /// repository as the service.
    pub fn poller(&self) -> MetricsPollerTask {
        MetricsPollerTask::new(self.repo.clone())
    }
}

impl Module for ContainerRuntimeModule {
    fn name(&self) -> &'static str {
        MODULE_NAME
    }

    fn migrations(&self) -> Vec<Migration> {
        self.migrations.clone()
    }
}
