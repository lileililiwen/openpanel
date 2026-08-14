//! Service manager composition module and migration registration.

use std::sync::Arc;

use openpanel_core::{AppContext, Migration, Module};

use super::{RealSystemCtl, ServiceActor, ServiceLister, SqliteServiceManagerRepository};

/// Stable service manager module name.
pub const MODULE_NAME: &str = "service_manager";

/// Service manager bounded-context composition root.
pub struct ServiceManagerModule {
    repo: Arc<SqliteServiceManagerRepository>,
    lister: Arc<ServiceLister>,
    actor: Arc<ServiceActor>,
    migrations: Vec<Migration>,
}

impl ServiceManagerModule {
    /// Compose the bounded context.
    pub async fn new(ctx: &AppContext) -> Self {
        let pool = ctx.db.pool().await;
        let repo = Arc::new(SqliteServiceManagerRepository::new(pool));
        let systemctl: Arc<dyn super::service::SystemCtl> = Arc::new(RealSystemCtl::new());
        let lister = Arc::new(ServiceLister::new(systemctl.clone()));
        let actor = Arc::new(ServiceActor::new(
            repo.clone(),
            ctx.audit.clone(),
            systemctl,
        ));
        Self {
            repo,
            lister,
            actor,
            migrations: vec![Migration {
                module: MODULE_NAME,
                version: "001".to_owned(),
                description: "service manager action history".to_owned(),
                sql: crate::migrations::SERVICE_MANAGER_V001.to_owned(),
            }],
        }
    }

    /// Shared lister.
    pub fn lister(&self) -> Arc<ServiceLister> {
        self.lister.clone()
    }

    /// Shared actor.
    pub fn actor(&self) -> Arc<ServiceActor> {
        self.actor.clone()
    }

    /// Shared repository.
    pub fn repo(&self) -> Arc<SqliteServiceManagerRepository> {
        self.repo.clone()
    }
}

impl Module for ServiceManagerModule {
    fn name(&self) -> &'static str {
        MODULE_NAME
    }

    fn migrations(&self) -> Vec<Migration> {
        self.migrations.clone()
    }
}
