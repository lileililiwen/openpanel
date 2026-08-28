//! Git deployment composition module and migration registration.

use std::sync::Arc;

use openpanel_core::{AppContext, Migration, Module};

use super::{DeployService, PreviewService, SqliteDeployRepository, SqlitePreviewRepository};

/// Stable git deployment module name.
pub const MODULE_NAME: &str = "git_deployment";

/// Git deployment bounded-context composition root.
pub struct GitDeploymentModule {
    repo: Arc<SqliteDeployRepository>,
    service: Arc<DeployService>,
    preview_repo: Arc<SqlitePreviewRepository>,
    preview_service: Arc<PreviewService>,
    migrations: Vec<Migration>,
}

impl GitDeploymentModule {
    /// Compose the bounded context.
    pub async fn new(ctx: &AppContext) -> Self {
        let pool = ctx.db.pool().await;
        let repo = Arc::new(SqliteDeployRepository::new(pool.clone()));
        let service = Arc::new(DeployService::new(repo.clone(), ctx.audit.clone()));
        let preview_repo = Arc::new(SqlitePreviewRepository::new(pool));
        let preview_service = Arc::new(PreviewService::new(
            preview_repo.clone(),
            ctx.audit.clone(),
            std::env::var("OPENPANEL_PREVIEW_BASE_DOMAIN")
                .unwrap_or_else(|_| "pr.example.com".to_string()),
            std::env::var("OPENPANEL_PREVIEW_TTL_HOURS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(72),
            std::env::var("OPENPANEL_PREVIEW_MAX_PER_REPO")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(3),
        ));
        Self {
            repo,
            service,
            preview_repo,
            preview_service,
            migrations: vec![
                Migration {
                    module: MODULE_NAME,
                    version: "001".to_owned(),
                    description: "git deployment: repos and deploy runs".to_owned(),
                    sql: crate::migrations::GIT_DEPLOYMENT_V001.to_owned(),
                },
                Migration {
                    module: MODULE_NAME,
                    version: "002".to_owned(),
                    description: "git deployment: preview deployments".to_owned(),
                    sql: crate::migrations::GIT_DEPLOYMENT_V002.to_owned(),
                },
            ],
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

    /// Shared preview service.
    pub fn preview_service(&self) -> Arc<PreviewService> {
        self.preview_service.clone()
    }

    /// Shared preview repository.
    pub fn preview_repo(&self) -> Arc<SqlitePreviewRepository> {
        self.preview_repo.clone()
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
