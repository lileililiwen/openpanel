//! `MonitoringModule` — composition-root wiring for the monitoring
//! bounded context.

use std::sync::{Arc, Mutex};

use openpanel_core::{AppContext, Migration, Module};

use super::{
    AlertConfig, AlertEvaluator, Collector, MonitoringCollectorTask, MonitoringService,
    SystemCollector, repo::SqliteSnapshotRepository,
};

/// Stable identifier for the monitoring module used in migration
/// bookkeeping and the `/api/v1/monitoring` route namespace.
pub const MODULE_NAME: &str = "monitoring";

/// monitoring bounded-context module: wires the service, repository,
/// and collector task.
pub struct MonitoringModule {
    service: Arc<MonitoringService>,
    migrations: Vec<Migration>,
    interval_secs: u64,
}

impl MonitoringModule {
    /// Construct the module using the real `SystemCollector`.
    pub async fn new(ctx: &AppContext) -> Self {
        let collector: Arc<Mutex<Box<dyn Collector>>> =
            Arc::new(Mutex::new(Box::new(SystemCollector::new())));
        Self::with_collector(ctx, collector).await
    }

    /// Construct with a caller-provided collector (tests inject a
    /// double).
    pub async fn with_collector(
        ctx: &AppContext,
        collector: Arc<Mutex<Box<dyn Collector>>>,
    ) -> Self {
        let pool = ctx.db.pool().await;
        let repo: Arc<dyn openpanel_domain::SnapshotRepository> =
            Arc::new(SqliteSnapshotRepository::new(pool));

        let monitoring_cfg = ctx.config.monitoring();
        let alert_config = AlertConfig {
            cpu_percent: monitoring_cfg
                .alert
                .get("cpu_percent")
                .and_then(|v| v.as_f64())
                .filter(|v| v.is_finite() && *v >= 0.0),
            memory_percent: monitoring_cfg
                .alert
                .get("memory_percent")
                .and_then(|v| v.as_f64())
                .filter(|v| v.is_finite() && *v >= 0.0),
            disk_percent: monitoring_cfg
                .alert
                .get("disk_percent")
                .and_then(|v| v.as_f64())
                .filter(|v| v.is_finite() && *v >= 0.0),
        };
        let evaluator = AlertEvaluator::new(&alert_config);

        let interval_secs = monitoring_cfg.interval_secs.max(1);
        let retention_days = monitoring_cfg.retention_days;

        let service = Arc::new(MonitoringService::new(
            repo,
            collector,
            ctx.audit.clone(),
            evaluator,
            retention_days,
        ));

        let migrations = vec![Migration {
            module: MODULE_NAME,
            version: "001".to_string(),
            description: "monitoring initial schema".to_string(),
            sql: crate::migrations::MONITORING_V001.to_string(),
        }];

        Self {
            service,
            migrations,
            interval_secs,
        }
    }

    /// Return a clone of the shared service handle.
    pub fn service(&self) -> Arc<MonitoringService> {
        self.service.clone()
    }
}

impl Module for MonitoringModule {
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
        vec![Box::new(MonitoringCollectorTask::new(
            self.service.clone(),
            self.interval_secs,
        ))]
    }
}
