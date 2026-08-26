//! Web-terminal application use cases.

use std::sync::Arc;

use chrono::Utc;
use openpanel_core::{AuditAction, AuditEvent, AuditOutcome, AuditService};
use openpanel_domain::{
    Role, Site, SiteRepository, User,
    web_terminal::{
        CloseReason, PtyPort, PtySpec, PtyStream, TICKET_TTL_SECS, TerminalError, TerminalSession,
        TerminalTicket, TicketRandomer, WebTerminalRepository,
    },
};
use uuid::Uuid;

/// Policy knobs for the browser terminal.
pub struct TerminalPolicy {
    /// Maximum simultaneously open sessions per user.
    pub max_sessions_per_user: u64,
    /// Whether the terminal accepts new sessions at all.
    pub enabled: bool,
    /// Unread PTY output budget before an overflow close.
    pub output_buffer_bytes: usize,
}

impl Default for TerminalPolicy {
    fn default() -> Self {
        Self {
            max_sessions_per_user: 1,
            enabled: true,
            output_buffer_bytes: 512 * 1024,
        }
    }
}

/// Owner/Admin orchestration for one-time tickets and scoped sessions.
pub struct WebTerminalService {
    repo: Arc<dyn WebTerminalRepository>,
    sites: Arc<dyn SiteRepository>,
    audit: Arc<dyn AuditService>,
    randomer: Arc<dyn TicketRandomer>,
    pty: Arc<dyn PtyPort>,
    policy: TerminalPolicy,
}

impl WebTerminalService {
    /// Construct with persistence, audit, randomness, and PTY ports.
    pub fn new(
        repo: Arc<dyn WebTerminalRepository>,
        sites: Arc<dyn SiteRepository>,
        audit: Arc<dyn AuditService>,
        randomer: Arc<dyn TicketRandomer>,
        pty: Arc<dyn PtyPort>,
        policy: TerminalPolicy,
    ) -> Self {
        Self {
            repo,
            sites,
            audit,
            randomer,
            pty,
            policy: TerminalPolicy {
                max_sessions_per_user: policy.max_sessions_per_user.max(1),
                enabled: policy.enabled,
                output_buffer_bytes: policy.output_buffer_bytes.max(1),
            },
        }
    }

    /// Maximum unread PTY output bytes tolerated before a session is
    /// force-closed with the `overflow` reason.
    pub fn output_buffer_bytes(&self) -> usize {
        self.policy.output_buffer_bytes
    }

    /// Mint a one-time ticket for a terminal on `site_id`.
    pub async fn issue_ticket(
        &self,
        caller: &User,
        site_id: Uuid,
    ) -> Result<TerminalTicket, TerminalError> {
        if !self.policy.enabled {
            return Err(TerminalError::Disabled);
        }
        if caller.role() != Role::Owner && caller.role() != Role::Admin {
            return Err(TerminalError::Forbidden);
        }
        if caller.disabled_at().is_some() {
            return Err(TerminalError::Suspended);
        }
        self.require_site(site_id).await?;
        let ticket = TerminalTicket::mint(
            self.randomer.as_ref(),
            caller.id(),
            site_id,
            Utc::now(),
            TICKET_TTL_SECS,
        );
        self.repo.put_ticket(&ticket).await.map_err(failure)?;
        Ok(ticket)
    }

    /// Consume a presented ticket (single use), enforce the per-user
    /// session cap, spawn the scoped PTY, and record the session.
    /// Returns the session id plus the live PTY stream.
    pub async fn start_session(
        &self,
        token: &str,
        site_user: &str,
        cwd: &str,
    ) -> Result<(Uuid, Box<dyn PtyStream>), TerminalError> {
        if !self.policy.enabled {
            return Err(TerminalError::Disabled);
        }
        let mut ticket = self
            .repo
            .take_ticket(token)
            .await
            .map_err(failure)?
            .ok_or(TerminalError::UnknownTicket)?;
        ticket.consume(Utc::now())?;
        self.repo.update_ticket(&ticket).await.map_err(failure)?;

        if self
            .repo
            .count_open_sessions(ticket.user_id)
            .await
            .map_err(failure)?
            >= self.policy.max_sessions_per_user
        {
            return Err(TerminalError::SessionCap);
        }

        let spec = PtySpec {
            user: site_user.to_owned(),
            cwd: cwd.to_owned(),
            shell: String::new(),
        };
        let stream = self.pty.open(&spec)?;
        let session = TerminalSession::open(ticket.user_id, ticket.site_id, Utc::now());
        self.repo.insert_session(&session).await.map_err(failure)?;
        let _ = self
            .audit
            .record(
                AuditEvent::new(
                    "web-terminal",
                    AuditAction::TerminalOpened,
                    AuditOutcome::Success,
                )
                .target(session.id.to_string())
                .metadata(serde_json::json!({
                    "user_id": ticket.user_id,
                    "site_id": ticket.site_id
                })),
            )
            .await;
        Ok((session.id, stream))
    }

    /// Close a session with the given reason and emit the
    /// `TerminalClosed` audit event. The event carries only the
    /// session coordinates — never PTY stream content.
    pub async fn close_session(
        &self,
        session_id: Uuid,
        reason: CloseReason,
    ) -> Result<(), TerminalError> {
        let closed_at = Utc::now();
        self.repo
            .close_session(session_id, reason, closed_at)
            .await
            .map_err(failure)?;
        if let Some(session) = self.repo.get_session(session_id).await.map_err(failure)? {
            let _ = self
                .audit
                .record(
                    AuditEvent::new(
                        "web-terminal",
                        AuditAction::TerminalClosed,
                        AuditOutcome::Success,
                    )
                    .target(session.id.to_string())
                    .metadata(serde_json::json!({
                        "user_id": session.user_id,
                        "site_id": session.site_id,
                        "opened_at": session.opened_at.to_rfc3339(),
                        "closed_at": closed_at.to_rfc3339(),
                        "reason": reason.as_str(),
                    })),
                )
                .await;
        }
        Ok(())
    }

    /// Resolve the PTY target (site user + working directory) for a
    /// site. v1 runs the shell as the site owner's username inside the
    /// site document root.
    pub async fn resolve_session_target(
        &self,
        site_id: Uuid,
    ) -> Result<(String, String), TerminalError> {
        let site = self.require_site(site_id).await?;
        Ok((
            format!("op-{}", site.created_by()),
            site.document_root().to_owned(),
        ))
    }

    /// Look up a ticket's session target without consuming it.
    pub async fn peek_ticket_target(&self, token: &str) -> Result<(String, String), TerminalError> {
        let ticket = self
            .repo
            .take_ticket(token)
            .await
            .map_err(failure)?
            .ok_or(TerminalError::UnknownTicket)?;
        self.resolve_session_target(ticket.site_id).await
    }

    async fn require_site(&self, site_id: Uuid) -> Result<Site, TerminalError> {
        self.sites
            .find_by_id(site_id)
            .await
            .map_err(failure)?
            .ok_or_else(|| TerminalError::SiteNotFound(site_id.to_string()))
    }
}

fn failure(error: impl std::fmt::Display) -> TerminalError {
    TerminalError::Failure(error.to_string())
}
