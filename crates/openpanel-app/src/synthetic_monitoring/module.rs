//! Synthetic monitoring composition module and migration registration.

use std::sync::Arc;

use openpanel_core::{AppContext, Migration, Module};

use super::{
    CheckRunner, ProbeScheduler, SqliteStatusPageRepository, SqliteSyntheticRepository,
    StatusPageService,
};

/// Stable synthetic monitoring module name.
pub const MODULE_NAME: &str = "synthetic_monitoring";

/// Synthetic monitoring bounded-context composition root.
pub struct SyntheticMonitoringModule {
    repo: Arc<SqliteSyntheticRepository>,
    runner: Arc<CheckRunner>,
    scheduler: Arc<ProbeScheduler>,
    status_page: Arc<StatusPageService>,
    migrations: Vec<Migration>,
}

impl SyntheticMonitoringModule {
    /// Compose the bounded context with the default probe ports.
    pub async fn new(
        ctx: &AppContext,
        http: Arc<dyn super::service::HttpProbe>,
        tcp: Arc<dyn super::service::TcpProbe>,
        ssl: Arc<dyn super::service::SslExpiryInspector>,
    ) -> Self {
        let pool = ctx.db.pool().await;
        let sql_repo = Arc::new(SqliteSyntheticRepository::new(pool));
        let runner = Arc::new(CheckRunner::new(
            sql_repo.clone(),
            http,
            tcp,
            ssl,
            ctx.audit.clone(),
        ));
        let scheduler = Arc::new(ProbeScheduler::new(sql_repo.clone(), runner.clone()));
        let status_repo = Arc::new(SqliteStatusPageRepository::new(ctx.db.pool().await));
        let status_page = Arc::new(StatusPageService::new(
            status_repo,
            sql_repo.clone(),
            ctx.audit.clone(),
        ));
        Self {
            repo: sql_repo.clone(),
            runner,
            scheduler,
            status_page,
            migrations: vec![
                Migration {
                    module: MODULE_NAME,
                    version: "001".to_owned(),
                    description: "synthetic checks and per-run results".to_owned(),
                    sql: crate::migrations::SYNTHETIC_MONITORING_V001.to_owned(),
                },
                Migration {
                    module: MODULE_NAME,
                    version: "002".to_owned(),
                    description: "status page policy + entry labels".to_owned(),
                    sql: crate::migrations::SYNTHETIC_MONITORING_V002.to_owned(),
                },
            ],
        }
    }

    /// Shared runner.
    pub fn runner(&self) -> Arc<CheckRunner> {
        self.runner.clone()
    }

    /// Shared scheduler.
    pub fn scheduler(&self) -> Arc<ProbeScheduler> {
        self.scheduler.clone()
    }

    /// Shared repository.
    pub fn repo(&self) -> Arc<SqliteSyntheticRepository> {
        self.repo.clone()
    }

    /// Shared status page service.
    pub fn status_page(&self) -> Arc<StatusPageService> {
        self.status_page.clone()
    }
}

impl Module for SyntheticMonitoringModule {
    fn name(&self) -> &'static str {
        MODULE_NAME
    }

    fn migrations(&self) -> Vec<Migration> {
        self.migrations.clone()
    }
}
