//! Periodic incremental traffic aggregation.

use std::sync::Arc;

use async_trait::async_trait;
use openpanel_core::jobs::BackgroundTask;
use tokio::sync::Notify;

use super::LogService;

pub(super) struct LogAggregationTask {
    pub(super) service: Arc<LogService>,
}

#[async_trait]
impl BackgroundTask for LogAggregationTask {
    fn name(&self) -> &'static str {
        "log-traffic-aggregation"
    }

    async fn run(self: Box<Self>, shutdown: Arc<Notify>) -> anyhow::Result<()> {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    if let Err(error) = self.service.aggregate_once().await {
                        tracing::warn!(error = %error, "log aggregation tick failed");
                    }
                }
                _ = shutdown.notified() => break,
            }
        }
        Ok(())
    }
}
