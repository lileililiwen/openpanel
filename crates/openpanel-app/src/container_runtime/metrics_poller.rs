//! Metrics poller/retention background task.
//!
//! The container runtime bounded context does not own the docker
//! container lifecycle (that is the `docker` bounded context).
//! The metrics poller is a retention-only task here: it walks
//! every container known by the metrics repository and trims
//! samples older than the configured retention boundary
//! (spec: "pruned at the 1-hour retention boundary"). The actual
//! sampling is performed by the `POST /containers/{id}/metrics`
//! endpoint or by an upstream docker-stats poller that calls
//! `ContainerRuntimeService::record_metrics`.

use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use openpanel_core::jobs::BackgroundTask;
use tokio::sync::Notify;

use super::repo::SqliteContainerRuntimeRepository;

/// Background task that prunes stale metrics samples.
pub struct MetricsPollerTask {
    repo: Arc<SqliteContainerRuntimeRepository>,
    interval_secs: u64,
    retention_secs: i64,
}

impl MetricsPollerTask {
    /// Construct a poller with a bounded tick interval and the
    /// retention boundary in seconds. Defaults: tick = 60s,
    /// retention = 3600s (1 hour, per the spec).
    pub fn new(repo: Arc<SqliteContainerRuntimeRepository>) -> Self {
        Self {
            repo,
            interval_secs: 60,
            retention_secs: 3600,
        }
    }

    /// Override the tick interval.
    pub fn with_interval(mut self, secs: u64) -> Self {
        self.interval_secs = secs.clamp(1, 3600);
        self
    }

    /// Override the retention boundary (seconds).
    pub fn with_retention(mut self, secs: i64) -> Self {
        self.retention_secs = secs.max(60);
        self
    }

    /// Run one retention pass against the repository.
    pub async fn prune_once(&self) -> anyhow::Result<()> {
        // `container_metrics_samples` is a time-series; prune by
        // `sampled_at < (now - retention)`. The repo doesn't expose
        // "list all distinct container_ids" (kept narrow on
        // purpose), so the poller issues a single bulk delete via
        // sqlx here.
        let cutoff: DateTime<Utc> = Utc::now() - chrono::Duration::seconds(self.retention_secs);
        let sql = "DELETE FROM container_metrics_samples WHERE sampled_at < ?";
        let pool = self.repo.pool();
        let res = sqlx::query(sql)
            .bind(cutoff.to_rfc3339())
            .execute(&pool)
            .await
            .map_err(|e| anyhow::anyhow!(e))?;
        let _ = res;
        Ok(())
    }
}

#[async_trait]
impl BackgroundTask for MetricsPollerTask {
    fn name(&self) -> &'static str {
        "container-runtime-metrics-poller"
    }

    async fn run(self: Box<Self>, shutdown: Arc<Notify>) -> anyhow::Result<()> {
        let mut interval = tokio::time::interval(Duration::from_secs(self.interval_secs));
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    if let Err(e) = self.prune_once().await {
                        tracing::warn!(%e, "container metrics retention prune failed");
                    }
                }
                _ = shutdown.notified() => break,
            }
        }
        Ok(())
    }
}
