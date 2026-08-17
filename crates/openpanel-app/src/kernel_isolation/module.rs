//! Kernel isolation composition module and migration registration.

use std::sync::Arc;

use openpanel_core::{AppContext, Migration, Module};

use super::{CgroupEnforcer, NamespaceIsolator, SqliteIsolationRepository};

/// Stable kernel isolation module name.
pub const MODULE_NAME: &str = "kernel_isolation";

/// Kernel isolation bounded-context composition root.
pub struct KernelIsolationModule {
    repo: Arc<SqliteIsolationRepository>,
    enforcer: Arc<CgroupEnforcer>,
    isolator: NamespaceIsolator,
    migrations: Vec<Migration>,
}

impl KernelIsolationModule {
    /// Compose the bounded context with the default cgroup writer.
    pub async fn new(ctx: &AppContext, writer: Arc<dyn super::service::CgroupWriter>) -> Self {
        let pool = ctx.db.pool().await;
        let repo = Arc::new(SqliteIsolationRepository::new(pool));
        let enforcer = Arc::new(CgroupEnforcer::new(writer, ctx.audit.clone()));
        let isolator = NamespaceIsolator::new();
        Self {
            repo,
            enforcer,
            isolator,
            migrations: vec![Migration {
                module: MODULE_NAME,
                version: "001".to_owned(),
                description: "kernel isolation: cgroup + namespace + policy".to_owned(),
                sql: crate::migrations::KERNEL_ISOLATION_V001.to_owned(),
            }],
        }
    }

    /// Shared enforcer.
    pub fn enforcer(&self) -> Arc<CgroupEnforcer> {
        self.enforcer.clone()
    }

    /// Shared isolator.
    pub fn isolator(&self) -> &NamespaceIsolator {
        &self.isolator
    }

    /// Shared repository.
    pub fn repo(&self) -> Arc<SqliteIsolationRepository> {
        self.repo.clone()
    }
}

impl Module for KernelIsolationModule {
    fn name(&self) -> &'static str {
        MODULE_NAME
    }

    fn migrations(&self) -> Vec<Migration> {
        self.migrations.clone()
    }
}
