//! Synthetic monitoring composition module and migration registration.

use std::sync::Arc;

use openpanel_core::{AppContext, Migration, Module};

use super::{CheckRunner, ProbeScheduler, SqliteSyntheticRepository};

/// Stable synthetic monitoring module name.
pub const MODULE_NAME: &str = "synthetic_monitoring";

/// Synthetic monitoring bounded-context composition root.
pub struct SyntheticMonitoringModule {
    repo: Arc<SqliteSyntheticRepository>,
    runner: Arc<CheckRunner>,
    scheduler: Arc<ProbeScheduler>,
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
        Self {
            repo: sql_repo.clone(),
            runner,
            scheduler,
            migrations: vec![Migration {
                module: MODULE_NAME,
                version: "001".to_owned(),
                description: "synthetic checks and per-run results".to_owned(),
                sql: crate::migrations::SYNTHETIC_MONITORING_V001.to_owned(),
            }],
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
}

impl Module for SyntheticMonitoringModule {
    fn name(&self) -> &'static str {
        MODULE_NAME
    }

    fn migrations(&self) -> Vec<Migration> {
        self.migrations.clone()
    }
}
