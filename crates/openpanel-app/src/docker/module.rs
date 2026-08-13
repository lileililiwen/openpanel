//! Docker composition root and migration registration.

use std::sync::Arc;

use openpanel_core::{AppContext, Migration, Module};
use openpanel_domain::docker::DockerRepository;

use super::{
    BollardDockerAdapter, DockerAdapter, DockerService, NetworkAdapter, SqliteDockerRepository,
};

/// Stable module name.
pub const MODULE_NAME: &str = "docker";

/// Docker bounded-context composition root.
pub struct DockerModule {
    service: Arc<DockerService>,
    migrations: Vec<Migration>,
}

impl DockerModule {
    /// Compose with the local Docker/Podman socket.
    pub async fn new(
        ctx: &AppContext,
        master_key: [u8; 32],
    ) -> Result<Self, openpanel_domain::docker::DockerError> {
        let runtime = Arc::new(BollardDockerAdapter::connect()?);
        runtime.ping().await?;
        let adapter: Arc<dyn DockerAdapter> = runtime.clone();
        let network: Arc<dyn NetworkAdapter> = runtime;
        Ok(Self::with_adapters(ctx, adapter, network, master_key).await)
    }

    /// Compose with an injected runtime adapter.
    pub async fn with_adapters(
        ctx: &AppContext,
        adapter: Arc<dyn DockerAdapter>,
        network: Arc<dyn NetworkAdapter>,
        master_key: [u8; 32],
    ) -> Self {
        let repo: Arc<dyn DockerRepository> =
            Arc::new(SqliteDockerRepository::new(ctx.db.pool().await, master_key));
        Self {
            service: Arc::new(DockerService::new(
                repo,
                adapter,
                network,
                ctx.audit.clone(),
            )),
            migrations: vec![Migration {
                module: MODULE_NAME,
                version: "001".into(),
                description: "container desired state and image trust".into(),
                sql: crate::migrations::DOCKER_V001.into(),
            }],
        }
    }

    /// Shared lifecycle service.
    pub fn service(&self) -> Arc<DockerService> {
        self.service.clone()
    }
}

impl Module for DockerModule {
    fn name(&self) -> &'static str {
        MODULE_NAME
    }

    fn migrations(&self) -> Vec<Migration> {
        self.migrations.clone()
    }

    fn background_tasks(
        &self,
        _ctx: &AppContext,
    ) -> Vec<Box<dyn openpanel_core::jobs::BackgroundTask>> {
        vec![Box::new(super::DockerReconcileTask::new(
            self.service.clone(),
            60,
        ))]
    }
}
