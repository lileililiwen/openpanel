//! Supervised notification dispatcher.

use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use openpanel_core::BackgroundTask;
use tokio::sync::Notify;

use super::NotificationService;

/// Periodically leases and delivers due notifications.
pub struct NotificationDispatcherTask {
    service: Arc<NotificationService>,
    tick_seconds: u64,
}

impl NotificationDispatcherTask {
    /// Construct with a nonzero interval.
    pub fn new(service: Arc<NotificationService>, tick_seconds: u64) -> Self {
        Self {
            service,
            tick_seconds: tick_seconds.max(1),
        }
    }
}

#[async_trait]
impl BackgroundTask for NotificationDispatcherTask {
    fn name(&self) -> &'static str {
        "notification-dispatcher"
    }

    async fn run(self: Box<Self>, shutdown: Arc<Notify>) -> anyhow::Result<()> {
        let mut interval = tokio::time::interval(Duration::from_secs(self.tick_seconds));
        loop {
            tokio::select! {
                _ = shutdown.notified() => return Ok(()),
                _ = interval.tick() => {
                    if let Err(error) = self.service.dispatch_once(chrono::Utc::now(), 100).await {
                        tracing::warn!(error = %error, "notification dispatch tick failed");
                    }
                }
            }
        }
    }
}
