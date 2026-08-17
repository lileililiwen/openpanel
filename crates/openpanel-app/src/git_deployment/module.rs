//! Git deployment composition module and migration registration.

use std::sync::Arc;

use openpanel_core::{AppContext, Migration, Module};

use super::{DeployService, SqliteDeployRepository};

/// Stable git deployment module name.
pub const MODULE_NAME: &str = "git_deployment";

/// Git deployment bounded-context composition root.
pub struct GitDeploymentModule {
    repo: Arc<SqliteDeployRepository>,
    service: Arc<DeployService>,
    migrations: Vec<Migration>,
}

impl GitDeploymentModule {
    /// Compose the bounded context.
    pub async fn new(ctx: &AppContext) -> Self {
        let pool = ctx.db.pool().await;
        let repo = Arc::new(SqliteDeployRepository::new(pool));
        let service = Arc::new(DeployService::new(repo.clone(), ctx.audit.clone()));
        Self {
            repo,
            service,
            migrations: vec![Migration {
                module: MODULE_NAME,
                version: "001".to_owned(),
                description: "git deployment: repos and deploy runs".to_owned(),
                sql: crate::migrations::GIT_DEPLOYMENT_V001.to_owned(),
            }],
        }
    }

    /// Shared service.
    pub fn service(&self) -> Arc<DeployService> {
        self.service.clone()
    }

    /// Shared repository.
    pub fn repo(&self) -> Arc<SqliteDeployRepository> {
        self.repo.clone()
    }
}

impl Module for GitDeploymentModule {
    fn name(&self) -> &'static str {
        MODULE_NAME
    }

    fn migrations(&self) -> Vec<Migration> {
        self.migrations.clone()
    }
}
