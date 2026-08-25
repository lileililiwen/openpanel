//! SQLite web-terminal repository adapter.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use openpanel_domain::{
    RepoError,
    web_terminal::{CloseReason, SessionState, TerminalTicket, WebTerminalRepository},
};
use sqlx::{Pool, Sqlite};
use uuid::Uuid;

/// SQLite-backed ticket and session store.
pub struct SqliteWebTerminalRepository {
    pool: Pool<Sqlite>,
}

impl SqliteWebTerminalRepository {
    /// Construct the adapter over a SQLite pool.
    pub fn new(pool: Pool<Sqlite>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl WebTerminalRepository for SqliteWebTerminalRepository {
    async fn put_ticket(&self, ticket: &TerminalTicket) -> Result<(), RepoError> {
        sqlx::query(
            "INSERT INTO web_terminal_tickets \
             (token, user_id, site_id, expires_at, consumed) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&ticket.token.0)
        .bind(ticket.user_id.to_string())
        .bind(ticket.site_id.to_string())
        .bind(ticket.expires_at.to_rfc3339())
        .bind(ticket.consumed)
        .execute(&self.pool)
        .await
        .map_err(repo_error)?;
        Ok(())
    }

    async fn take_ticket(&self, token: &str) -> Result<Option<TerminalTicket>, RepoError> {
        let row = sqlx::query_as::<_, (String, String, String, String, bool)>(
            "SELECT token, user_id, site_id, expires_at, consumed \
             FROM web_terminal_tickets WHERE token = ?",
        )
        .bind(token)
        .fetch_optional(&self.pool)
        .await
        .map_err(repo_error)?;
        row.map(|(token, user_id, site_id, expires_at, consumed)| {
            Ok(TerminalTicket {
                token: openpanel_domain::web_terminal::Token(token),
                user_id: Uuid::parse_str(&user_id).map_err(repo_error)?,
                site_id: Uuid::parse_str(&site_id).map_err(repo_error)?,
                expires_at: DateTime::parse_from_rfc3339(&expires_at)
                    .map_err(repo_error)?
                    .with_timezone(&Utc),
                consumed,
            })
        })
        .transpose()
    }

    async fn update_ticket(&self, ticket: &TerminalTicket) -> Result<(), RepoError> {
        sqlx::query("UPDATE web_terminal_tickets SET consumed = ? WHERE token = ?")
            .bind(ticket.consumed)
            .bind(&ticket.token.0)
            .execute(&self.pool)
            .await
            .map_err(repo_error)?;
        Ok(())
    }

    async fn count_open_sessions(&self, user_id: Uuid) -> Result<u64, RepoError> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM web_terminal_sessions \
             WHERE user_id = ? AND state = 'open'",
        )
        .bind(user_id.to_string())
        .fetch_one(&self.pool)
        .await
        .map_err(repo_error)?;
        u64::try_from(count).map_err(repo_error)
    }

    async fn insert_session(
        &self,
        session: &openpanel_domain::web_terminal::TerminalSession,
    ) -> Result<(), RepoError> {
        sqlx::query(
            "INSERT INTO web_terminal_sessions \
             (id, user_id, site_id, opened_at, closed_at, state, close_reason) \
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(session.id.to_string())
        .bind(session.user_id.to_string())
        .bind(session.site_id.to_string())
        .bind(session.opened_at.to_rfc3339())
        .bind(session.closed_at.map(|at| at.to_rfc3339()))
        .bind(match session.state {
            SessionState::Open => "open",
            SessionState::Closed => "closed",
        })
        .bind(session.close_reason.map(reason_str))
        .execute(&self.pool)
        .await
        .map_err(repo_error)?;
        Ok(())
    }

    async fn close_session(
        &self,
        id: Uuid,
        reason: CloseReason,
        at: DateTime<Utc>,
    ) -> Result<(), RepoError> {
        sqlx::query(
            "UPDATE web_terminal_sessions \
             SET state = 'closed', closed_at = ?, close_reason = ? \
             WHERE id = ? AND state = 'open'",
        )
        .bind(at.to_rfc3339())
        .bind(reason_str(reason))
        .bind(id.to_string())
        .execute(&self.pool)
        .await
        .map_err(repo_error)?;
        Ok(())
    }

    async fn prune_tickets(&self, cutoff: DateTime<Utc>) -> Result<u64, RepoError> {
        let result = sqlx::query("DELETE FROM web_terminal_tickets WHERE expires_at < ?")
            .bind(cutoff.to_rfc3339())
            .execute(&self.pool)
            .await
            .map_err(repo_error)?;
        Ok(result.rows_affected())
    }
}

fn reason_str(reason: CloseReason) -> &'static str {
    match reason {
        CloseReason::ClientClosed => "client_closed",
        CloseReason::IdleTimeout => "idle_timeout",
        CloseReason::Overflow => "overflow",
    }
}

fn repo_error(error: impl std::fmt::Display) -> RepoError {
    RepoError::new(error.to_string())
}
