//! Supervised compose desired-state reconciliation.

use std::sync::Arc;

use async_trait::async_trait;
use openpanel_core::jobs::BackgroundTask;
use tokio::sync::Notify;

use super::DockerService;

/// Re-applies signed desired state within the configured drift window.
pub struct DockerReconcileTask {
    service: Arc<DockerService>,
    interval_secs: u64,
}

impl DockerReconcileTask {
    /// Construct a reconciler with a bounded tick interval.
    pub fn new(service: Arc<DockerService>, interval_secs: u64) -> Self {
        Self {
            service,
            interval_secs,
        }
    }
}

#[async_trait]
impl BackgroundTask for DockerReconcileTask {
    fn name(&self) -> &'static str {
        "docker-compose-reconciler"
    }

    async fn run(self: Box<Self>, shutdown: Arc<Notify>) -> anyhow::Result<()> {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(
            self.interval_secs.clamp(1, 60),
        ));
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    if let Err(error) = self.service.reconcile_stacks().await {
                        tracing::warn!(%error, "Docker compose reconciliation failed");
                    }
                }
                _ = shutdown.notified() => break,
            }
        }
        Ok(())
    }
}
