//! SQLite adapter for the AI Ops bounded context.

use std::str::FromStr;

use chrono::{DateTime, Utc};
use openpanel_domain::{
    AiAction, AiActionId, AiActionStatus, AiMessage, AiOpsRepository, AiSession, AiSessionId,
    MessageRole, RepoError, ToolKind, ToolName, ToolResult,
};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

/// SQLite-backed `AiOpsRepository`.
#[derive(Clone)]
pub struct SqliteAiOpsRepository {
    pool: SqlitePool,
}

impl SqliteAiOpsRepository {
    /// Construct a repository over the shared SQLite pool.
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl AiOpsRepository for SqliteAiOpsRepository {
    async fn save_session(&self, session: &AiSession) -> Result<(), RepoError> {
        sqlx::query(
            "INSERT OR REPLACE INTO ai_sessions (id, owner_id, title, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(session.id.to_string())
        .bind(session.owner.to_string())
        .bind(&session.title)
        .bind(session.created_at.to_rfc3339())
        .bind(session.updated_at.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(())
    }

    async fn get_session(&self, id: AiSessionId) -> Result<Option<AiSession>, RepoError> {
        let row = sqlx::query(
            "SELECT id, owner_id, title, created_at, updated_at FROM ai_sessions WHERE id = ?",
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(row.map(decode_session).transpose()?)
    }

    async fn list_sessions(&self, owner: Uuid, limit: u32) -> Result<Vec<AiSession>, RepoError> {
        let rows = sqlx::query(
            "SELECT id, owner_id, title, created_at, updated_at \
             FROM ai_sessions WHERE owner_id = ? ORDER BY updated_at DESC LIMIT ?",
        )
        .bind(owner.to_string())
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        rows.into_iter().map(decode_session).collect()
    }

    async fn save_message(&self, message: &AiMessage) -> Result<(), RepoError> {
        let tool_json = serde_json::to_string(&message.tool_results)
            .map_err(|e| RepoError::new(e.to_string()))?;
        sqlx::query(
            "INSERT OR REPLACE INTO ai_messages (id, session_id, role, content, tool_results_json, created_at) \
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(message.id.to_string())
        .bind(message.session_id.to_string())
        .bind(message.role.as_str())
        .bind(&message.content)
        .bind(tool_json)
        .bind(message.created_at.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(())
    }

    async fn list_messages(&self, session_id: AiSessionId) -> Result<Vec<AiMessage>, RepoError> {
        let rows = sqlx::query(
            "SELECT id, session_id, role, content, tool_results_json, created_at \
             FROM ai_messages WHERE session_id = ? ORDER BY created_at ASC",
        )
        .bind(session_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        rows.into_iter().map(decode_message).collect()
    }

    async fn save_action(&self, action: &AiAction) -> Result<(), RepoError> {
        let params =
            serde_json::to_string(&action.params).map_err(|e| RepoError::new(e.to_string()))?;
        sqlx::query(
            "INSERT OR REPLACE INTO ai_actions (id, session_id, tool_name, kind, params_json, \
             status, approved_by, audit_id, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(action.id.to_string())
        .bind(action.session_id.to_string())
        .bind(action.tool.as_str())
        .bind(action.kind.as_str())
        .bind(params)
        .bind(action.status.as_str())
        .bind(action.approved_by.map(|u| u.to_string()))
        .bind(action.audit_id.map(|u| u.to_string()))
        .bind(action.created_at.to_rfc3339())
        .bind(action.updated_at.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(())
    }

    async fn get_action(&self, id: AiActionId) -> Result<Option<AiAction>, RepoError> {
        let row = sqlx::query(
            "SELECT id, session_id, tool_name, kind, params_json, status, approved_by, \
             audit_id, created_at, updated_at FROM ai_actions WHERE id = ?",
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        row.map(decode_action).transpose()
    }

    async fn update_action(&self, action: &AiAction) -> Result<(), RepoError> {
        let params =
            serde_json::to_string(&action.params).map_err(|e| RepoError::new(e.to_string()))?;
        sqlx::query(
            "UPDATE ai_actions SET status = ?, approved_by = ?, audit_id = ?, updated_at = ?, \
             params_json = ? WHERE id = ?",
        )
        .bind(action.status.as_str())
        .bind(action.approved_by.map(|u| u.to_string()))
        .bind(action.audit_id.map(|u| u.to_string()))
        .bind(action.updated_at.to_rfc3339())
        .bind(params)
        .bind(action.id.to_string())
        .execute(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(())
    }

    async fn list_pending_actions(&self, limit: u32) -> Result<Vec<AiAction>, RepoError> {
        let rows = sqlx::query(
            "SELECT id, session_id, tool_name, kind, params_json, status, approved_by, \
             audit_id, created_at, updated_at FROM ai_actions \
             WHERE status = 'proposed' ORDER BY created_at ASC LIMIT ?",
        )
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        rows.into_iter().map(decode_action).collect()
    }

    async fn list_actions(&self, session_id: AiSessionId) -> Result<Vec<AiAction>, RepoError> {
        let rows = sqlx::query(
            "SELECT id, session_id, tool_name, kind, params_json, status, approved_by, \
             audit_id, created_at, updated_at FROM ai_actions \
             WHERE session_id = ? ORDER BY created_at ASC",
        )
        .bind(session_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        rows.into_iter().map(decode_action).collect()
    }
}

fn decode_session(row: sqlx::sqlite::SqliteRow) -> Result<AiSession, RepoError> {
    let id_str: String = row.try_get("id").map_err(map_sqlx)?;
    let owner_str: String = row.try_get("owner_id").map_err(map_sqlx)?;
    let title: String = row.try_get("title").map_err(map_sqlx)?;
    let created_at: String = row.try_get("created_at").map_err(map_sqlx)?;
    let updated_at: String = row.try_get("updated_at").map_err(map_sqlx)?;
    let id =
        Uuid::parse_str(&id_str).map_err(|e| RepoError::new(format!("invalid session id: {e}")))?;
    let owner = Uuid::parse_str(&owner_str)
        .map_err(|e| RepoError::new(format!("invalid owner id: {e}")))?;
    Ok(AiSession {
        id: AiSessionId(id),
        owner,
        title,
        created_at: parse_timestamp_utc(&created_at)?,
        updated_at: parse_timestamp_utc(&updated_at)?,
    })
}

fn decode_message(row: sqlx::sqlite::SqliteRow) -> Result<AiMessage, RepoError> {
    let id_str: String = row.try_get("id").map_err(map_sqlx)?;
    let session_str: String = row.try_get("session_id").map_err(map_sqlx)?;
    let role: String = row.try_get("role").map_err(map_sqlx)?;
    let content: String = row.try_get("content").map_err(map_sqlx)?;
    let tool_json: String = row.try_get("tool_results_json").map_err(map_sqlx)?;
    let created_at: String = row.try_get("created_at").map_err(map_sqlx)?;
    let id =
        Uuid::parse_str(&id_str).map_err(|e| RepoError::new(format!("invalid message id: {e}")))?;
    let session = Uuid::parse_str(&session_str)
        .map_err(|e| RepoError::new(format!("invalid session id: {e}")))?;
    let tool_results: Vec<ToolResult> = serde_json::from_str(&tool_json)
        .map_err(|e| RepoError::new(format!("invalid tool results: {e}")))?;
    let role = match role.as_str() {
        "user" => MessageRole::User,
        "assistant" => MessageRole::Assistant,
        "system" => MessageRole::System,
        other => return Err(RepoError::new(format!("unknown message role: {other}"))),
    };
    Ok(AiMessage {
        id,
        session_id: AiSessionId(session),
        role,
        content,
        tool_results,
        created_at: parse_timestamp_utc(&created_at)?,
    })
}

fn decode_action(row: sqlx::sqlite::SqliteRow) -> Result<AiAction, RepoError> {
    let id_str: String = row.try_get("id").map_err(map_sqlx)?;
    let session_str: String = row.try_get("session_id").map_err(map_sqlx)?;
    let tool_name: String = row.try_get("tool_name").map_err(map_sqlx)?;
    let kind: String = row.try_get("kind").map_err(map_sqlx)?;
    let params_json: String = row.try_get("params_json").map_err(map_sqlx)?;
    let status: String = row.try_get("status").map_err(map_sqlx)?;
    let approved_by: Option<String> = row.try_get("approved_by").map_err(map_sqlx)?;
    let audit_id: Option<String> = row.try_get("audit_id").map_err(map_sqlx)?;
    let created_at: String = row.try_get("created_at").map_err(map_sqlx)?;
    let updated_at: String = row.try_get("updated_at").map_err(map_sqlx)?;

    let id = AiActionId(
        Uuid::parse_str(&id_str).map_err(|e| RepoError::new(format!("invalid action id: {e}")))?,
    );
    let session = Uuid::parse_str(&session_str)
        .map_err(|e| RepoError::new(format!("invalid session id: {e}")))?;
    let tool = ToolName::new(tool_name.clone())
        .ok_or_else(|| RepoError::new(format!("invalid tool name: {tool_name}")))?;
    let kind = match kind.as_str() {
        "read" => ToolKind::Read,
        "write" => ToolKind::Write,
        other => return Err(RepoError::new(format!("unknown action kind: {other}"))),
    };
    let params: serde_json::Value = serde_json::from_str(&params_json)
        .map_err(|e| RepoError::new(format!("invalid action params: {e}")))?;
    let status = match status.as_str() {
        "proposed" => AiActionStatus::Proposed,
        "executed" => AiActionStatus::Executed,
        "denied" => AiActionStatus::Denied,
        other => return Err(RepoError::new(format!("unknown action status: {other}"))),
    };
    let approved_by = approved_by
        .map(|s| Uuid::parse_str(&s))
        .transpose()
        .map_err(|e| RepoError::new(format!("invalid approver: {e}")))?;
    let audit_id = audit_id
        .map(|s| Uuid::parse_str(&s))
        .transpose()
        .map_err(|e| RepoError::new(format!("invalid audit id: {e}")))?;
    Ok(AiAction {
        id,
        session_id: AiSessionId(session),
        tool,
        kind,
        params,
        status,
        approved_by,
        audit_id,
        created_at: parse_timestamp_utc(&created_at)?,
        updated_at: parse_timestamp_utc(&updated_at)?,
    })
}

fn map_sqlx(e: sqlx::Error) -> RepoError {
    RepoError::new(e.to_string())
}

/// Convert an RFC 3339 string to a `DateTime<Utc>`.
pub fn parse_timestamp_utc(s: &str) -> Result<DateTime<Utc>, RepoError> {
    DateTime::from_str(s).map_err(|e| RepoError::new(format!("invalid timestamp: {e}")))
}
