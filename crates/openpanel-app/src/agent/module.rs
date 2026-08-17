//! Agent module: composition root for the bounded context.

use std::sync::Arc;

use openpanel_core::{AppContext, Migration, Module};

use crate::agent::{AgentService, SqliteAgentRepository};

/// Stable module name.
pub const MODULE_NAME: &str = "agent";

/// Agent module wires the SQLite adapter, the application service,
/// and the migration.
pub struct AgentModule {
    service: Arc<AgentService>,
    migrations: Vec<Migration>,
}

impl AgentModule {
    /// Build the module from the panel application context.
    pub async fn new(ctx: &AppContext) -> Self {
        let pool = ctx.db.pool().await;
        let repo = Arc::new(SqliteAgentRepository::new(pool));
        let service = Arc::new(AgentService::new(repo, ctx.audit.clone()));
        Self {
            service,
            migrations: vec![Migration {
                module: MODULE_NAME,
                version: "001".to_string(),
                description: "agent registry, fleet tokens, recipe manifests".to_string(),
                sql: crate::migrations::AGENT_V001.to_string(),
            }],
        }
    }

    /// Return the shared service handle.
    pub fn service(&self) -> Arc<AgentService> {
        self.service.clone()
    }
}

impl Module for AgentModule {
    fn name(&self) -> &'static str {
        MODULE_NAME
    }

    fn migrations(&self) -> Vec<Migration> {
        self.migrations.clone()
    }

    fn config_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {}
        })
    }
}
