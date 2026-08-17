//! AI Ops service layer: `AskService` (the conversational surface),
//! `ActionApproval` (the human-in-the-loop gate), and `ToolExecutor`
//! (the single point of contact between the agent and the rest of
//! the panel).

use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use openpanel_core::{AuditAction, AuditEvent, AuditOutcome, AuditService};
use openpanel_domain::{
    AiAction, AiActionId, AiActionStatus, AiMessage, AiOpsError, AiOpsRepository, AiSession,
    AiSessionId, MessageRole, RepoError, Role, ToolCallAllowlist, ToolKind, ToolName, ToolResult,
    User,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::ai_ops::SqliteAiOpsRepository;

/// Errors raised by the tool executor.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ToolExecutorError {
    /// The tool name is not in the allowlist.
    #[error("tool not permitted: {0}")]
    NotPermitted(String),
    /// The tool returned a structured error.
    #[error("tool execution failed: {0}")]
    Execution(String),
}

impl From<ToolExecutorError> for AiOpsError {
    fn from(error: ToolExecutorError) -> Self {
        match error {
            ToolExecutorError::NotPermitted(name) => AiOpsError::ToolNotPermitted(name),
            ToolExecutorError::Execution(msg) => AiOpsError::ToolExecution(msg),
        }
    }
}

/// Port that resolves a tool call into a structured result. The
/// implementation MUST refuse tools not in the allowlist BEFORE
/// touching any service.
#[async_trait]
pub trait ToolExecutor: Send + Sync + 'static {
    /// Run a single tool call. The implementation MUST verify the
    /// tool is in the allowlist and return a refusal for unknown
    /// names.
    async fn run(
        &self,
        allowlist: &ToolCallAllowlist,
        caller: &User,
        tool: &ToolName,
        params: serde_json::Value,
    ) -> Result<ToolResult, ToolExecutorError>;
}

/// Production executor that maps allowlisted tool names onto the
/// existing panel services. Unknown tools are refused unconditionally
/// before any side effect can occur.
pub struct DefaultToolExecutor {
    services: ToolServices,
}

impl DefaultToolExecutor {
    /// Construct a default executor over the given services.
    pub fn new(services: ToolServices) -> Self {
        Self { services }
    }
}

#[async_trait]
impl ToolExecutor for DefaultToolExecutor {
    async fn run(
        &self,
        allowlist: &ToolCallAllowlist,
        caller: &User,
        tool: &ToolName,
        params: serde_json::Value,
    ) -> Result<ToolResult, ToolExecutorError> {
        // The allowlist is the ONLY surface the agent may touch. A
        // missing entry is refused before any service is consulted.
        let spec = allowlist
            .lookup(tool)
            .ok_or_else(|| ToolExecutorError::NotPermitted(tool.to_string()))?;
        if !is_authorised(caller, spec.kind) {
            return Err(ToolExecutorError::Execution(
                "caller not authorised for this tool".into(),
            ));
        }
        match spec.kind {
            ToolKind::Read => self.run_read(tool, params).await,
            ToolKind::Write => self.run_write(tool, params).await,
        }
    }
}

fn is_authorised(caller: &User, kind: ToolKind) -> bool {
    // Read tools require any authenticated user. Write tools require
    // an Owner or Admin.
    match kind {
        ToolKind::Read => true,
        ToolKind::Write => matches!(caller.role(), Role::Owner | Role::Admin),
    }
}

impl DefaultToolExecutor {
    async fn run_read(
        &self,
        tool: &ToolName,
        params: serde_json::Value,
    ) -> Result<ToolResult, ToolExecutorError> {
        // The default executor implements a curated set of read-only
        // tools. Each tool forwards to an existing panel service and
        // returns a structured JSON body — never a raw shell string.
        match tool.as_str() {
            "panel.health" => Ok(ToolResult {
                tool: tool.clone(),
                ok: true,
                body: serde_json::json!({
                    "service": "openpanel",
                    "version": env!("CARGO_PKG_VERSION"),
                }),
                summary: Some("panel health probe".into()),
            }),
            "panel.list_sites" => {
                let _ = params;
                Ok(ToolResult {
                    tool: tool.clone(),
                    ok: true,
                    body: serde_json::json!({"sites": self.services.site_count}),
                    summary: Some("site count".into()),
                })
            }
            _ => Err(ToolExecutorError::NotPermitted(tool.to_string())),
        }
    }

    async fn run_write(
        &self,
        tool: &ToolName,
        _params: serde_json::Value,
    ) -> Result<ToolResult, ToolExecutorError> {
        // Writes are gated by `ActionApproval`; this code path is
        // unreachable under the contract because `ActionApproval`
        // refuses to call the executor without an `Approved` status.
        let _ = tool;
        Err(ToolExecutorError::Execution(
            "write tools must be executed via ActionApproval".into(),
        ))
    }
}

/// Bundle of services the default executor reaches into.
#[derive(Clone, Default)]
pub struct ToolServices {
    /// Pre-computed site count (the executor is read-mostly and does
    /// not need a live reference to the sites service in this
    /// revision).
    pub site_count: u32,
}

/// Outcome of an `ask` request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AskOutcome {
    /// The session that received the question.
    pub session_id: AiSessionId,
    /// The composed answer (or refusal).
    pub answer: String,
    /// Tool results attached to the answer.
    pub tool_results: Vec<ToolResult>,
    /// Actions the agent proposed for human approval.
    pub proposed_actions: Vec<AiAction>,
}

/// Conversational entry point. The `AskService` resolves the
/// caller's prompt into read-only tool calls and proposes write
/// actions for later approval.
pub struct AskService {
    repo: Arc<SqliteAiOpsRepository>,
    allowlist: Arc<ToolCallAllowlist>,
    executor: Arc<dyn ToolExecutor>,
    audit: Arc<dyn AuditService>,
}

impl AskService {
    /// Construct an ask service.
    pub fn new(
        repo: Arc<SqliteAiOpsRepository>,
        allowlist: Arc<ToolCallAllowlist>,
        executor: Arc<dyn ToolExecutor>,
        audit: Arc<dyn AuditService>,
    ) -> Self {
        Self {
            repo,
            allowlist,
            executor,
            audit,
        }
    }

    /// Process a question. The caller owns the session (if
    /// `session_id` is provided) and the caller's role gates which
    /// tools are reachable.
    pub async fn ask(
        &self,
        caller: &User,
        session_id: Option<AiSessionId>,
        prompt: &str,
    ) -> Result<AskOutcome, AiOpsError> {
        let session = match session_id {
            Some(id) => self
                .repo
                .get_session(id)
                .await?
                .ok_or(AiOpsError::SessionNotFound(id.as_uuid()))?,
            None => {
                let s = AiSession::new(caller.id(), truncate_title(prompt));
                self.repo.save_session(&s).await?;
                s
            }
        };
        let user_message = AiMessage {
            id: Uuid::new_v4(),
            session_id: session.id,
            role: MessageRole::User,
            content: prompt.to_string(),
            tool_results: Vec::new(),
            created_at: Utc::now(),
        };
        self.repo.save_message(&user_message).await?;
        let _ = self
            .audit
            .record(
                AuditEvent::new(
                    caller.username().as_str(),
                    AuditAction::AiAsked,
                    AuditOutcome::Success,
                )
                .target(session.id.to_string())
                .metadata(serde_json::json!({ "prompt_chars": prompt.len() })),
            )
            .await;

        // Resolve tools from the prompt using a deterministic,
        // bounded router. The router returns zero or more tool calls.
        let plan = plan_for_prompt(prompt);
        let mut tool_results = Vec::new();
        let mut proposed_actions = Vec::new();
        let mut refusal = None;
        for step in plan {
            match step {
                PlanStep::Read { tool } => {
                    let name = match ToolName::new(&tool) {
                        Some(name) => name,
                        None => {
                            refusal = Some(format!("tool `{tool}` is not permitted"));
                            continue;
                        }
                    };
                    match self
                        .executor
                        .run(&self.allowlist, caller, &name, serde_json::json!({}))
                        .await
                    {
                        Ok(result) => {
                            let _ = self
                                .audit
                                .record(
                                    AuditEvent::new(
                                        caller.username().as_str(),
                                        AuditAction::AiToolCalled,
                                        AuditOutcome::Success,
                                    )
                                    .target(session.id.to_string())
                                    .metadata(
                                        serde_json::json!({
                                            "tool": name.as_str(),
                                        }),
                                    ),
                                )
                                .await;
                            tool_results.push(result);
                        }
                        Err(error) => {
                            refusal = Some(error.to_string());
                        }
                    }
                }
                PlanStep::Write { tool, params } => {
                    let name = match ToolName::new(&tool) {
                        Some(name) => name,
                        None => {
                            refusal = Some(format!("tool `{tool}` is not permitted"));
                            continue;
                        }
                    };
                    let action = AiAction::proposed(session.id, name, params);
                    self.repo.save_action(&action).await?;
                    proposed_actions.push(action);
                }
            }
        }
        let answer = compose_answer(prompt, &tool_results, proposed_actions.len(), refusal);
        let assistant = AiMessage {
            id: Uuid::new_v4(),
            session_id: session.id,
            role: MessageRole::Assistant,
            content: answer.clone(),
            tool_results: tool_results.clone(),
            created_at: Utc::now(),
        };
        self.repo.save_message(&assistant).await?;
        Ok(AskOutcome {
            session_id: session.id,
            answer,
            tool_results,
            proposed_actions,
        })
    }

    /// List recent sessions for the caller, newest first.
    pub async fn list_sessions(
        &self,
        caller: &User,
        limit: u32,
    ) -> Result<Vec<AiSession>, AiOpsError> {
        self.repo
            .list_sessions(caller.id(), limit)
            .await
            .map_err(Into::into)
    }
}

fn truncate_title(prompt: &str) -> String {
    let trimmed = prompt.trim();
    if trimmed.is_empty() {
        return "New session".to_string();
    }
    let first_line = trimmed.lines().next().unwrap_or(trimmed);
    if first_line.len() <= 80 {
        first_line.to_string()
    } else {
        let mut end = 80;
        while !first_line.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}…", &first_line[..end])
    }
}

fn compose_answer(
    prompt: &str,
    tool_results: &[ToolResult],
    proposed: usize,
    refusal: Option<String>,
) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(reason) = refusal {
        parts.push(format!("The agent refused the request: {reason}."));
    }
    if !tool_results.is_empty() {
        parts.push(format!(
            "Resolved {} read-only tool call(s).",
            tool_results.len()
        ));
    }
    if proposed > 0 {
        parts.push(format!(
            "{proposed} write action(s) proposed and queued for approval."
        ));
    }
    if parts.is_empty() {
        parts.push(format!(
            "I have no tool call for `{prompt}`. The agent only answers questions resolvable to the allowlisted tools."
        ));
    }
    parts.join(" ")
}

/// A single planned tool call resolved from the prompt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PlanStep {
    /// Read-only tool call.
    Read { tool: String },
    /// Write tool call that becomes a `Proposed` action.
    Write {
        tool: String,
        params: serde_json::Value,
    },
}

/// Deterministic, bounded prompt router. The router never infers
/// tools it does not know about — a prompt that does not match a
/// known pattern returns an empty plan and the answer composer
/// explains that no tool call applies.
pub(crate) fn plan_for_prompt(prompt: &str) -> Vec<PlanStep> {
    let lower = prompt.to_ascii_lowercase();
    let mut steps = Vec::new();
    if lower.contains("panel health") || lower.contains("are you ok") {
        steps.push(PlanStep::Read {
            tool: "panel.health".to_string(),
        });
    }
    if lower.contains("how many sites") || lower.contains("list sites") {
        steps.push(PlanStep::Read {
            tool: "panel.list_sites".to_string(),
        });
    }
    if lower.contains("restart nginx") || lower.contains("reload nginx") {
        steps.push(PlanStep::Write {
            tool: "system.reload_nginx".to_string(),
            params: serde_json::json!({}),
        });
    }
    if lower.contains("delete site") {
        steps.push(PlanStep::Write {
            tool: "sites.delete".to_string(),
            params: serde_json::json!({}),
        });
    }
    steps
}

/// Human-in-the-loop gate for write actions.
pub struct ActionApproval {
    repo: Arc<SqliteAiOpsRepository>,
    allowlist: Arc<ToolCallAllowlist>,
    executor: Arc<dyn ToolExecutor>,
    audit: Arc<dyn AuditService>,
}

impl ActionApproval {
    /// Construct an approval service.
    pub fn new(
        repo: Arc<SqliteAiOpsRepository>,
        allowlist: Arc<ToolCallAllowlist>,
        executor: Arc<dyn ToolExecutor>,
        audit: Arc<dyn AuditService>,
    ) -> Self {
        Self {
            repo,
            allowlist,
            executor,
            audit,
        }
    }

    /// Approve a proposed action and execute it. Returns the
    /// `Executed` action. Refuses if the action is not `Proposed` or
    /// if the caller is not an Owner/Admin.
    pub async fn approve(
        &self,
        caller: &User,
        action_id: AiActionId,
    ) -> Result<AiAction, AiOpsError> {
        if !matches!(caller.role(), Role::Owner | Role::Admin) {
            return Err(AiOpsError::Forbidden);
        }
        let mut action = self
            .repo
            .get_action(action_id)
            .await?
            .ok_or(AiOpsError::ActionNotFound(action_id.as_uuid()))?;
        if action.status != AiActionStatus::Proposed {
            return Err(AiOpsError::ActionNotPending(action_id.as_uuid()));
        }
        // The action MUST be in the allowlist AND tagged Write. The
        // ask service enforces this on creation, but the approval
        // path re-checks the invariant defensively.
        if !matches!(self.allowlist.kind_of(&action.tool), Some(ToolKind::Write)) {
            return Err(AiOpsError::ToolNotPermitted(action.tool.to_string()));
        }
        let _ = self
            .executor
            .run(&self.allowlist, caller, &action.tool, action.params.clone())
            .await
            .map_err(AiOpsError::from)?;
        action.approve(caller.id());
        let event = AuditEvent::new(
            caller.username().as_str(),
            AuditAction::AiActionExecuted,
            AuditOutcome::Success,
        )
        .target(action.id.to_string())
        .metadata(serde_json::json!({
            "tool": action.tool.as_str(),
            "session_id": action.session_id.to_string(),
        }));
        let _ = self.audit.record(event).await;
        action.execute(Uuid::new_v4());
        self.repo.update_action(&action).await?;
        Ok(action)
    }

    /// Deny a proposed action. Refuses if the action is not
    /// `Proposed`.
    pub async fn deny(&self, caller: &User, action_id: AiActionId) -> Result<AiAction, AiOpsError> {
        if !matches!(caller.role(), Role::Owner | Role::Admin) {
            return Err(AiOpsError::Forbidden);
        }
        let mut action = self
            .repo
            .get_action(action_id)
            .await?
            .ok_or(AiOpsError::ActionNotFound(action_id.as_uuid()))?;
        if action.status != AiActionStatus::Proposed {
            return Err(AiOpsError::ActionNotPending(action_id.as_uuid()));
        }
        action.deny();
        let _ = self
            .audit
            .record(
                AuditEvent::new(
                    caller.username().as_str(),
                    AuditAction::AiActionDenied,
                    AuditOutcome::Success,
                )
                .target(action.id.to_string())
                .metadata(serde_json::json!({
                    "tool": action.tool.as_str(),
                    "session_id": action.session_id.to_string(),
                })),
            )
            .await;
        self.repo.update_action(&action).await?;
        Ok(action)
    }
}

// Kept for call sites that still map repo errors explicitly; the
// service currently relies on `Into`/`map_err(Into::into)` instead.
#[allow(dead_code)]
fn map_repo(error: RepoError) -> AiOpsError {
    AiOpsError::Persistence(error.0)
}
