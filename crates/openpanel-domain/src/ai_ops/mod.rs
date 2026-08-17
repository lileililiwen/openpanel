//! AI Ops bounded context: `AiSession`, `AiMessage`, `AiAction`, and the
//! tool-call allowlist that gates every executable surface the agent
//! may touch.
//!
//! The agent is a **read-mostly** tool. Every tool call must resolve
//! to a name in the [`ToolCallAllowlist`]; absent names are refused
//! unconditionally. Read-only tools may execute automatically;
//! `Write`-kind tools MUST enter the `Proposed` state and wait for a
//! human approval before reaching `Executed`. The agent never spawns
//! a shell and never sees an arbitrary command string — it only sees
//! typed tool names plus a serialised parameter object that the
//! executor forwards to the relevant service.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::RepoError;

/// Errors raised by the AI Ops bounded context.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AiOpsError {
    /// The caller is not authorised to use the AI Ops surface.
    #[error("forbidden")]
    Forbidden,
    /// The referenced session does not exist.
    #[error("session not found: {0}")]
    SessionNotFound(Uuid),
    /// The referenced action does not exist.
    #[error("action not found: {0}")]
    ActionNotFound(Uuid),
    /// The tool the agent requested is not in the allowlist.
    #[error("tool not permitted: {0}")]
    ToolNotPermitted(String),
    /// The action is not in a state that can be approved/denied.
    #[error("action is not pending approval: {0}")]
    ActionNotPending(Uuid),
    /// Persistence failed.
    #[error("persistence failed: {0}")]
    Persistence(String),
    /// Tool execution failed.
    #[error("tool execution failed: {0}")]
    ToolExecution(String),
}

impl From<AiOpsError> for RepoError {
    fn from(error: AiOpsError) -> Self {
        RepoError::new(error.to_string())
    }
}

impl From<RepoError> for AiOpsError {
    fn from(error: RepoError) -> Self {
        AiOpsError::Persistence(error.0)
    }
}

/// Stable session identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AiSessionId(pub Uuid);

impl AiSessionId {
    /// Construct a new session id.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Underlying UUID.
    pub fn as_uuid(&self) -> Uuid {
        self.0
    }
}

impl Default for AiSessionId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for AiSessionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Stable action identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AiActionId(pub Uuid);

impl AiActionId {
    /// Construct a new action id.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Underlying UUID.
    pub fn as_uuid(&self) -> Uuid {
        self.0
    }
}

impl Default for AiActionId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for AiActionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Stable, opaque tool name (matches `^[a-z][a-z0-9_.]{1,63}$`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ToolName(String);

impl ToolName {
    /// Construct a validated tool name. Returns `None` if the name
    /// does not match the contract vocabulary.
    pub fn new(name: impl Into<String>) -> Option<Self> {
        let name = name.into();
        if name.is_empty() || name.len() > 64 {
            return None;
        }
        let bytes = name.as_bytes();
        if !bytes[0].is_ascii_lowercase() {
            return None;
        }
        if !bytes
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'.' || *b == b'_')
        {
            return None;
        }
        Some(Self(name))
    }

    /// Underlying name.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for ToolName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Whether a tool is read-only or mutating.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolKind {
    /// Tool may execute automatically without approval.
    Read,
    /// Tool requires human approval before execution.
    Write,
}

impl ToolKind {
    /// Stable lower-case label.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Write => "write",
        }
    }
}

/// Description of a single tool the agent may invoke.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolSpec {
    /// Stable tool name.
    pub name: ToolName,
    /// Read or write classification.
    pub kind: ToolKind,
    /// Human-readable description shown in the approval queue.
    pub description: String,
}

/// Allowlist enumerating every tool the agent may invoke.
///
/// Adding a tool to the allowlist is the ONLY way to expose
/// functionality. There is no `run_command` tool and no fallback path.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ToolCallAllowlist {
    tools: Vec<ToolSpec>,
}

impl ToolCallAllowlist {
    /// Construct an empty allowlist.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a tool to the allowlist. Returns `false` if a tool with
    /// the same name already exists (the first registration wins).
    pub fn register(&mut self, spec: ToolSpec) -> bool {
        if self.tools.iter().any(|t| t.name == spec.name) {
            return false;
        }
        self.tools.push(spec);
        true
    }

    /// Look up a tool by name.
    pub fn lookup(&self, name: &ToolName) -> Option<&ToolSpec> {
        self.tools.iter().find(|t| &t.name == name)
    }

    /// All registered tools.
    pub fn tools(&self) -> &[ToolSpec] {
        &self.tools
    }

    /// The `Read`/`Write` kind of a tool, or `None` if the name is
    /// not in the allowlist.
    pub fn kind_of(&self, name: &ToolName) -> Option<ToolKind> {
        self.lookup(name).map(|spec| spec.kind)
    }
}

/// Lifecycle state of an `AiAction`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiActionStatus {
    /// A write action was proposed and is waiting for approval.
    Proposed,
    /// A write action was approved and has been executed.
    Executed,
    /// A write action was denied by an authorised principal.
    Denied,
}

impl AiActionStatus {
    /// Stable lower-case label.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Proposed => "proposed",
            Self::Executed => "executed",
            Self::Denied => "denied",
        }
    }
}

/// A proposed, approved, executed, or denied tool call.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiAction {
    /// Stable action id.
    pub id: AiActionId,
    /// Session this action belongs to.
    pub session_id: AiSessionId,
    /// The tool the agent wants to invoke.
    pub tool: ToolName,
    /// Read or write classification (mirrors the allowlist).
    pub kind: ToolKind,
    /// Strictly typed parameters forwarded to the executor.
    pub params: serde_json::Value,
    /// Current lifecycle state.
    pub status: AiActionStatus,
    /// Principal that approved the action (write actions only).
    pub approved_by: Option<Uuid>,
    /// Audit entry recorded on execution.
    pub audit_id: Option<Uuid>,
    /// When the action was created.
    pub created_at: DateTime<Utc>,
    /// When the action transitioned to its current state.
    pub updated_at: DateTime<Utc>,
}

impl AiAction {
    /// Construct a `Proposed` write action.
    pub fn proposed(session_id: AiSessionId, tool: ToolName, params: serde_json::Value) -> Self {
        let now = Utc::now();
        Self {
            id: AiActionId::new(),
            session_id,
            tool,
            kind: ToolKind::Write,
            params,
            status: AiActionStatus::Proposed,
            approved_by: None,
            audit_id: None,
            created_at: now,
            updated_at: now,
        }
    }

    /// Mark the action as approved and set the approver.
    pub fn approve(&mut self, approver: Uuid) {
        self.approved_by = Some(approver);
        self.updated_at = Utc::now();
    }

    /// Mark the action as executed and record the audit id.
    pub fn execute(&mut self, audit_id: Uuid) {
        self.status = AiActionStatus::Executed;
        self.audit_id = Some(audit_id);
        self.updated_at = Utc::now();
    }

    /// Mark the action as denied.
    pub fn deny(&mut self) {
        self.status = AiActionStatus::Denied;
        self.updated_at = Utc::now();
    }
}

/// A user-visible question or answer recorded against a session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiMessage {
    /// Stable message id.
    pub id: Uuid,
    /// Session the message belongs to.
    pub session_id: AiSessionId,
    /// `user` for human input, `assistant` for agent output.
    pub role: MessageRole,
    /// Visible body of the message (no secrets, no raw commands).
    pub content: String,
    /// Tool results attached to this message, in the order the
    /// agent invoked them.
    #[serde(default)]
    pub tool_results: Vec<ToolResult>,
    /// When the message was recorded.
    pub created_at: DateTime<Utc>,
}

/// Role of a message in the conversation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageRole {
    /// Question from the human caller.
    User,
    /// Answer or status from the agent.
    Assistant,
    /// System-side marker (e.g. approval record).
    System,
}

impl MessageRole {
    /// Stable lower-case label.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Assistant => "assistant",
            Self::System => "system",
        }
    }
}

/// Result of executing one tool call. The body is a JSON object the
/// caller can render or pass downstream; the agent never embeds raw
/// shell output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolResult {
    /// Tool that produced this result.
    pub tool: ToolName,
    /// Whether the tool ran successfully.
    pub ok: bool,
    /// The tool's structured response (read) or summary (write).
    pub body: serde_json::Value,
    /// Optional human-readable summary for the answer composer.
    #[serde(default)]
    pub summary: Option<String>,
}

/// A conversational session owned by a single user.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiSession {
    /// Stable session id.
    pub id: AiSessionId,
    /// Owning user.
    pub owner: Uuid,
    /// Session title (defaults to the first question).
    pub title: String,
    /// When the session was created.
    pub created_at: DateTime<Utc>,
    /// When the session was last updated.
    pub updated_at: DateTime<Utc>,
}

impl AiSession {
    /// Construct a new session owned by `owner` with the given title.
    pub fn new(owner: Uuid, title: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: AiSessionId::new(),
            owner,
            title: title.into(),
            created_at: now,
            updated_at: now,
        }
    }

    /// Touch the session's `updated_at` timestamp.
    pub fn touch(&mut self) {
        self.updated_at = Utc::now();
    }
}

/// Persistence port for the AI Ops bounded context.
#[async_trait]
pub trait AiOpsRepository: Send + Sync + 'static {
    /// Persist a new session.
    async fn save_session(&self, session: &AiSession) -> Result<(), RepoError>;
    /// Load a session by id.
    async fn get_session(&self, id: AiSessionId) -> Result<Option<AiSession>, RepoError>;
    /// List recent sessions, newest first.
    async fn list_sessions(&self, owner: Uuid, limit: u32) -> Result<Vec<AiSession>, RepoError>;

    /// Persist a message against a session.
    async fn save_message(&self, message: &AiMessage) -> Result<(), RepoError>;
    /// Load all messages for a session, oldest first.
    async fn list_messages(&self, session_id: AiSessionId) -> Result<Vec<AiMessage>, RepoError>;

    /// Persist a new action.
    async fn save_action(&self, action: &AiAction) -> Result<(), RepoError>;
    /// Load an action by id.
    async fn get_action(&self, id: AiActionId) -> Result<Option<AiAction>, RepoError>;
    /// Update an action (approve, execute, deny).
    async fn update_action(&self, action: &AiAction) -> Result<(), RepoError>;
    /// List pending actions across all sessions.
    async fn list_pending_actions(&self, limit: u32) -> Result<Vec<AiAction>, RepoError>;
    /// List actions for a session.
    async fn list_actions(&self, session_id: AiSessionId) -> Result<Vec<AiAction>, RepoError>;
}
