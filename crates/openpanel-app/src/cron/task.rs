//! Supervised due-job scheduler.

use std::sync::Arc;

use async_trait::async_trait;
use openpanel_core::jobs::BackgroundTask;
use tokio::sync::Notify;

use super::CronService;

/// Drives persistent cron leases on a short interval.
pub struct CronSchedulerTask {
    service: Arc<CronService>,
    interval_secs: u64,
    retention_days: i64,
}

impl CronSchedulerTask {
    /// Construct a scheduler task.
    pub fn new(service: Arc<CronService>, interval_secs: u64, retention_days: i64) -> Self {
        Self {
            service,
            interval_secs,
            retention_days,
        }
    }
}

#[async_trait]
impl BackgroundTask for CronSchedulerTask {
    fn name(&self) -> &'static str {
        "cron-scheduler"
    }

    async fn run(self: Box<Self>, shutdown: Arc<Notify>) -> anyhow::Result<()> {
        let mut interval =
            tokio::time::interval(std::time::Duration::from_secs(self.interval_secs.max(1)));
        loop {
            tokio::select! { _ = interval.tick() => { if let Err(error) = self.service.tick(chrono::Utc::now(), self.retention_days).await { tracing::warn!(%error, "cron scheduler tick failed"); } }, _ = shutdown.notified() => break }
        }
        Ok(())
    }
}
