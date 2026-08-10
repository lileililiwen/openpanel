//! System-services module composition.
use std::{path::PathBuf, sync::Arc, time::Duration};

use async_trait::async_trait;
use openpanel_core::{AppContext, BackgroundTask, Migration, Module};
use openpanel_domain::system_services::{ServiceAction, ServiceDescriptor, ServiceId};

use super::{
    EmptyJournal, HealthSupervisor, JournalctlAdapter, MemoryServiceController, ServiceManager,
    ServiceManagerError, SqliteServiceHealthRepository, SystemdController,
};
/// Stable module name.
pub const MODULE_NAME: &str = "system-services";
/// System-service manager, health supervisor, and schema.
pub struct SystemServicesModule {
    service: Arc<ServiceManager>,
    migrations: Vec<Migration>,
    supervisor: Arc<HealthSupervisor>,
}
impl SystemServicesModule {
    /// Compose production systemd and journal adapters.
    pub async fn new(ctx: &AppContext) -> Result<Self, ServiceManagerError> {
        let binary = std::env::var("OPENPANEL__SERVICES__CONTROLLER")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("/usr/bin/systemctl"));
        let journal = std::env::var("OPENPANEL__SERVICES__JOURNAL")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("/usr/bin/journalctl"));
        Self::compose(
            ctx,
            Arc::new(SystemdController::new(binary, Duration::from_secs(15))),
            Arc::new(JournalctlAdapter::new(journal, Duration::from_secs(10))),
        )
        .await
    }

    /// Compose deterministic adapters for integration tests.
    pub async fn memory(ctx: &AppContext) -> Result<Self, ServiceManagerError> {
        Self::compose(
            ctx,
            Arc::new(MemoryServiceController::default()),
            Arc::new(EmptyJournal),
        )
        .await
    }

    async fn compose(
        ctx: &AppContext,
        controller: Arc<dyn super::ServiceController>,
        journal: Arc<dyn super::JournalPort>,
    ) -> Result<Self, ServiceManagerError> {
        let descriptors = descriptors()?;
        let repo = Arc::new(SqliteServiceHealthRepository::new(ctx.db.pool().await));
        let supervisor = Arc::new(HealthSupervisor::new(
            descriptors.clone(),
            controller.clone(),
            repo.clone(),
            ctx.audit.clone(),
            3,
            false,
            3,
            300,
        )?);
        Ok(Self {
            service: Arc::new(ServiceManager::new(
                descriptors,
                controller,
                journal,
                repo,
                ctx.audit.clone(),
            )),
            migrations: vec![Migration {
                module: MODULE_NAME,
                version: "001".into(),
                description: "service health history".into(),
                sql: crate::migrations::SYSTEM_SERVICES_V001.into(),
            }],
            supervisor,
        })
    }

    /// Shared application service.
    pub fn service(&self) -> Arc<ServiceManager> {
        self.service.clone()
    }
}
struct ServiceHealthTask {
    supervisor: Arc<HealthSupervisor>,
}
#[async_trait]
impl BackgroundTask for ServiceHealthTask {
    fn name(&self) -> &'static str {
        "system-service-health"
    }

    async fn run(self: Box<Self>, shutdown: Arc<tokio::sync::Notify>) -> anyhow::Result<()> {
        let mut interval = tokio::time::interval(Duration::from_secs(30));
        loop {
            tokio::select! {_ = shutdown.notified()=>break,_=interval.tick()=>{let now=u64::try_from(chrono::Utc::now().timestamp()).unwrap_or(0);if let Err(error)=self.supervisor.tick(now).await{tracing::warn!(%error,"system service health tick failed");}}}
        }
        Ok(())
    }
}
fn descriptors() -> Result<Vec<ServiceDescriptor>, ServiceManagerError> {
    Ok(vec![
        ServiceDescriptor::new(
            ServiceId::new("nginx").map_err(|_| ServiceManagerError::Controller)?,
            "Nginx",
            "nginx.service",
            vec![
                ServiceAction::Start,
                ServiceAction::Stop,
                ServiceAction::Restart,
                ServiceAction::Reload,
                ServiceAction::Enable,
                ServiceAction::Disable,
            ],
            vec!["sites".into(), "ssl".into()],
        )
        .map_err(|_| ServiceManagerError::Controller)?,
        ServiceDescriptor::new(
            ServiceId::new("mysql").map_err(|_| ServiceManagerError::Controller)?,
            "MySQL",
            "mysql.service",
            vec![
                ServiceAction::Start,
                ServiceAction::Stop,
                ServiceAction::Restart,
                ServiceAction::Reload,
                ServiceAction::Enable,
                ServiceAction::Disable,
            ],
            vec!["databases".into()],
        )
        .map_err(|_| ServiceManagerError::Controller)?,
        ServiceDescriptor::new(
            ServiceId::new("mariadb").map_err(|_| ServiceManagerError::Controller)?,
            "MariaDB",
            "mariadb.service",
            vec![
                ServiceAction::Start,
                ServiceAction::Stop,
                ServiceAction::Restart,
                ServiceAction::Reload,
                ServiceAction::Enable,
                ServiceAction::Disable,
            ],
            vec!["databases".into()],
        )
        .map_err(|_| ServiceManagerError::Controller)?,
        ServiceDescriptor::new(
            ServiceId::new("redis").map_err(|_| ServiceManagerError::Controller)?,
            "Redis",
            "redis-server.service",
            vec![
                ServiceAction::Start,
                ServiceAction::Stop,
                ServiceAction::Restart,
                ServiceAction::Enable,
                ServiceAction::Disable,
            ],
            vec!["sites".into()],
        )
        .map_err(|_| ServiceManagerError::Controller)?,
        ServiceDescriptor::new(
            ServiceId::new("php-83").map_err(|_| ServiceManagerError::Controller)?,
            "PHP-FPM 8.3",
            "php8.3-fpm.service",
            vec![
                ServiceAction::Start,
                ServiceAction::Stop,
                ServiceAction::Restart,
                ServiceAction::Reload,
                ServiceAction::Enable,
                ServiceAction::Disable,
            ],
            vec!["sites".into()],
        )
        .map_err(|_| ServiceManagerError::Controller)?,
        ServiceDescriptor::new(
            ServiceId::new("php-84").map_err(|_| ServiceManagerError::Controller)?,
            "PHP-FPM 8.4",
            "php8.4-fpm.service",
            vec![
                ServiceAction::Start,
                ServiceAction::Stop,
                ServiceAction::Restart,
                ServiceAction::Reload,
                ServiceAction::Enable,
                ServiceAction::Disable,
            ],
            vec!["sites".into()],
        )
        .map_err(|_| ServiceManagerError::Controller)?,
    ])
}
impl Module for SystemServicesModule {
    fn name(&self) -> &'static str {
        MODULE_NAME
    }

    fn migrations(&self) -> Vec<Migration> {
        self.migrations.clone()
    }

    fn background_tasks(&self, _ctx: &AppContext) -> Vec<Box<dyn BackgroundTask>> {
        vec![Box::new(ServiceHealthTask {
            supervisor: self.supervisor.clone(),
        })]
    }
}
