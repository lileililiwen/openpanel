//! Notification bounded-context composition root.

use std::sync::Arc;

use openpanel_core::{AppContext, Migration, Module};
use openpanel_domain::notifications::NotificationRepository;

use super::{
    NotificationAdapter, NotificationDispatcherTask, NotificationService,
    RustlsNotificationAdapter, SqliteNotificationRepository,
};

/// Stable module name.
pub const MODULE_NAME: &str = "notifications";

/// Notification module.
pub struct NotificationModule {
    service: Arc<NotificationService>,
    migrations: Vec<Migration>,
    tick_seconds: u64,
}

impl NotificationModule {
    /// Compose production rustls transports.
    pub async fn new(ctx: &AppContext, master_key: [u8; 32]) -> Result<Self, String> {
        let adapter: Arc<dyn NotificationAdapter> = Arc::new(RustlsNotificationAdapter::new()?);
        Ok(Self::with_adapter(ctx, master_key, adapter).await)
    }

    /// Compose with an injected adapter for tests.
    pub async fn with_adapter(
        ctx: &AppContext,
        master_key: [u8; 32],
        adapter: Arc<dyn NotificationAdapter>,
    ) -> Self {
        let repo: Arc<dyn NotificationRepository> =
            Arc::new(SqliteNotificationRepository::new(ctx.db.pool().await));
        let tick_seconds = std::env::var("OPENPANEL__NOTIFICATIONS__TICK_SECS")
            .ok()
            .and_then(|value| value.parse().ok())
            .filter(|value| *value > 0)
            .unwrap_or(10);
        Self {
            service: Arc::new(NotificationService::new(
                repo,
                adapter,
                ctx.audit.clone(),
                master_key,
            )),
            migrations: vec![Migration {
                module: MODULE_NAME,
                version: "001".into(),
                description: "durable notification delivery".into(),
                sql: crate::migrations::NOTIFICATIONS_V001.into(),
            }],
            tick_seconds,
        }
    }

    /// Shared use-case service.
    pub fn service(&self) -> Arc<NotificationService> {
        self.service.clone()
    }
}

impl Module for NotificationModule {
    fn name(&self) -> &'static str {
        MODULE_NAME
    }

    fn migrations(&self) -> Vec<Migration> {
        self.migrations.clone()
    }

    fn background_tasks(&self, _ctx: &AppContext) -> Vec<Box<dyn openpanel_core::BackgroundTask>> {
        vec![Box::new(NotificationDispatcherTask::new(
            self.service.clone(),
            self.tick_seconds,
        ))]
    }
}
