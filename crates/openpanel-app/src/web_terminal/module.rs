//! Web-terminal composition module and migration registration.

use std::sync::Arc;

use openpanel_core::{AppContext, Migration, Module};
use openpanel_domain::{
    SiteRepository,
    web_terminal::{OsTicketRandomer, PtyPort, WebTerminalRepository},
};

use super::{PortablePtyAdapter, SqliteWebTerminalRepository, WebTerminalService};
use crate::sites::repo::SqliteSiteRepository;

/// Stable module name.
pub const MODULE_NAME: &str = "web_terminal";

/// Default unread-output budget before a session is overflow-closed.
pub const DEFAULT_OUTPUT_BUFFER_BYTES: usize = 512 * 1024;

/// Web-terminal bounded-context composition root.
pub struct WebTerminalModule {
    service: Arc<WebTerminalService>,
    migrations: Vec<Migration>,
}

impl WebTerminalModule {
    /// Compose SQLite persistence and the host PTY adapter.
    pub async fn new(ctx: &AppContext) -> Self {
        Self::with_pty(ctx, Arc::new(PortablePtyAdapter::new("/bin/bash"))).await
    }

    /// Compose SQLite persistence with a caller-supplied PTY adapter
    /// (test doubles inject here).
    pub async fn with_pty(ctx: &AppContext, pty: Arc<dyn PtyPort>) -> Self {
        let pool = ctx.db.pool().await;
        let repo: Arc<dyn WebTerminalRepository> =
            Arc::new(SqliteWebTerminalRepository::new(pool.clone()));
        let sites: Arc<dyn SiteRepository> = Arc::new(SqliteSiteRepository::new(pool));
        let enabled = ctx
            .config
            .module_config("web_terminal")
            .and_then(|value| value.get("enabled"))
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(true);
        let max_sessions = ctx
            .config
            .module_config("web_terminal")
            .and_then(|value| value.get("max_sessions_per_user"))
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(1);
        let output_buffer_bytes = ctx
            .config
            .module_config("web_terminal")
            .and_then(|value| value.get("output_buffer_bytes"))
            .and_then(serde_json::Value::as_u64)
            .map(|value| value as usize)
            .unwrap_or(DEFAULT_OUTPUT_BUFFER_BYTES);
        let service = Arc::new(WebTerminalService::new(
            repo,
            sites,
            ctx.audit.clone(),
            Arc::new(OsTicketRandomer),
            pty,
            super::TerminalPolicy {
                max_sessions_per_user: max_sessions,
                enabled,
                output_buffer_bytes,
            },
        ));
        Self {
            service,
            migrations: vec![Migration {
                module: MODULE_NAME,
                version: "001".to_owned(),
                description: "terminal tickets and session records".to_owned(),
                sql: crate::migrations::WEB_TERMINAL_V001.to_owned(),
            }],
        }
    }

    /// Shared service.
    pub fn service(&self) -> Arc<WebTerminalService> {
        self.service.clone()
    }
}

impl Module for WebTerminalModule {
    fn name(&self) -> &'static str {
        MODULE_NAME
    }

    fn migrations(&self) -> Vec<Migration> {
        self.migrations.clone()
    }
}
