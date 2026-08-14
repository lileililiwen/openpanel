//! AI Ops composition module and migration registration.

use std::sync::Arc;

use openpanel_core::{AppContext, AuditService, Migration, Module};

use super::{
    ActionApproval, AskService, DefaultToolExecutor, SqliteAiOpsRepository, ToolExecutor,
    ToolServices,
};
use openpanel_domain::ToolCallAllowlist;

/// Stable AI Ops module name.
pub const MODULE_NAME: &str = "ai_ops";

/// Default allowlist shipped with the panel. New tools can be added
/// at runtime through the configuration loader, but the defaults are
/// the only ones the agent can call without explicit re-registration.
pub fn default_allowlist() -> ToolCallAllowlist {
    use openpanel_domain::ToolKind;
    let mut allowlist = ToolCallAllowlist::new();
    allowlist.register(openpanel_domain::ToolSpec {
        name: openpanel_domain::ToolName::new("panel.health").expect("static tool name"),
        kind: ToolKind::Read,
        description: "Return the panel version and a health probe.".into(),
    });
    allowlist.register(openpanel_domain::ToolSpec {
        name: openpanel_domain::ToolName::new("panel.list_sites").expect("static tool name"),
        kind: ToolKind::Read,
        description: "Return the number of provisioned sites.".into(),
    });
    allowlist
}

/// AI Ops bounded-context composition root.
pub struct AiOpsModule {
    repo: Arc<SqliteAiOpsRepository>,
    ask: Arc<AskService>,
    approval: Arc<ActionApproval>,
    allowlist: Arc<ToolCallAllowlist>,
    migrations: Vec<Migration>,
}

impl AiOpsModule {
    /// Compose the bounded context with the default allowlist and
    /// the default executor.
    pub async fn new(ctx: &AppContext) -> Self {
        let pool = ctx.db.pool().await;
        let repo = Arc::new(SqliteAiOpsRepository::new(pool));
        let allowlist = Arc::new(default_allowlist());
        let executor: Arc<dyn ToolExecutor> = Arc::new(DefaultToolExecutor::new(ToolServices::default()));
        let ask = Arc::new(AskService::new(
            repo.clone(),
            allowlist.clone(),
            executor.clone(),
            ctx.audit.clone(),
        ));
        let approval = Arc::new(ActionApproval::new(
            repo.clone(),
            allowlist.clone(),
            executor,
            ctx.audit.clone(),
        ));
        Self {
            repo,
            ask,
            approval,
            allowlist,
            migrations: vec![Migration {
                module: MODULE_NAME,
                version: "001".to_owned(),
                description: "AI Ops sessions, messages, and actions".to_owned(),
                sql: crate::migrations::AI_OPS_V001.to_owned(),
            }],
        }
    }

    /// Construct a module with a custom executor — used by tests.
    pub async fn with_executor(
        ctx: &AppContext,
        executor: Arc<dyn ToolExecutor>,
        audit: Arc<dyn AuditService>,
        allowlist: Arc<ToolCallAllowlist>,
    ) -> Self {
        let pool = ctx.db.pool().await;
        let repo = Arc::new(SqliteAiOpsRepository::new(pool));
        let ask = Arc::new(AskService::new(
            repo.clone(),
            allowlist.clone(),
            executor.clone(),
            audit.clone(),
        ));
        let approval = Arc::new(ActionApproval::new(
            repo.clone(),
            allowlist.clone(),
            executor,
            audit,
        ));
        Self {
            repo,
            ask,
            approval,
            allowlist,
            migrations: vec![Migration {
                module: MODULE_NAME,
                version: "001".to_owned(),
                description: "AI Ops sessions, messages, and actions".to_owned(),
                sql: crate::migrations::AI_OPS_V001.to_owned(),
            }],
        }
    }

    /// Shared ask service.
    pub fn ask(&self) -> Arc<AskService> {
        self.ask.clone()
    }

    /// Shared approval service.
    pub fn approval(&self) -> Arc<ActionApproval> {
        self.approval.clone()
    }

    /// Shared repository.
    pub fn repo(&self) -> Arc<SqliteAiOpsRepository> {
        self.repo.clone()
    }

    /// Shared allowlist.
    pub fn allowlist(&self) -> Arc<ToolCallAllowlist> {
        self.allowlist.clone()
    }
}

impl Module for AiOpsModule {
    fn name(&self) -> &'static str {
        MODULE_NAME
    }

    fn migrations(&self) -> Vec<Migration> {
        self.migrations.clone()
    }
}
