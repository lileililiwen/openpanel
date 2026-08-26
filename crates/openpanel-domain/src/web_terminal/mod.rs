//! Browser terminal: one-time tickets, scoped PTY sessions, and the
//! transport ports. Pure domain — no I/O.

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::RepoError;

/// Default ticket time-to-live.
pub const TICKET_TTL_SECS: i64 = 30;

/// Errors raised by the web-terminal bounded context.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TerminalError {
    /// The caller is not allowed to open a terminal.
    #[error("forbidden")]
    Forbidden,
    /// The requested site does not exist.
    #[error("site not found: {0}")]
    SiteNotFound(String),
    /// The account is suspended and may not open terminals.
    #[error("account suspended")]
    Suspended,
    /// The ticket is unknown.
    #[error("unknown ticket")]
    UnknownTicket,
    /// The ticket was already consumed.
    #[error("ticket already used")]
    Used,
    /// The ticket expired before consumption.
    #[error("ticket expired")]
    Expired,
    /// The per-user concurrent-session cap is reached.
    #[error("too many open sessions")]
    SessionCap,
    /// The terminal feature is disabled by configuration.
    #[error("web terminal disabled")]
    Disabled,
    /// PTY or persistence failure.
    #[error("terminal failure: {0}")]
    Failure(String),
}

/// Randomness port so ticket generation is testable without I/O.
pub trait TicketRandomer: Send + Sync + 'static {
    /// Fill `dest` with cryptographically strong random bytes.
    fn fill(&self, dest: &mut [u8]);
}

/// Production randomness backed by the `rand` crate.
pub struct OsTicketRandomer;

impl TicketRandomer for OsTicketRandomer {
    fn fill(&self, dest: &mut [u8]) {
        rand::rngs::OsRng.fill_bytes(dest);
    }
}

/// Lower-case hex encoding of a 256-bit token (64 chars).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Token(pub String);

/// Generate a fresh single-use ticket token.
pub fn new_token(randomer: &dyn TicketRandomer) -> Token {
    let mut bytes = [0u8; 32];
    randomer.fill(&mut bytes);
    let hex: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
    Token(hex)
}

/// A one-time, short-lived credential presented on the WebSocket
/// upgrade. Consumed exactly once; expiry is checked at consumption.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminalTicket {
    /// Single-use bearer token (64 lowercase hex chars).
    pub token: Token,
    /// Minting user.
    pub user_id: Uuid,
    /// Target site.
    pub site_id: Uuid,
    /// Instant after which consumption is refused.
    pub expires_at: DateTime<Utc>,
    /// Whether the ticket has been consumed already.
    pub consumed: bool,
}

impl TerminalTicket {
    /// Mint a ticket for `user`/`site` expiring after `ttl_secs`.
    pub fn mint(
        randomer: &dyn TicketRandomer,
        user_id: Uuid,
        site_id: Uuid,
        now: DateTime<Utc>,
        ttl_secs: i64,
    ) -> Self {
        Self {
            token: new_token(randomer),
            user_id,
            site_id,
            expires_at: now + Duration::seconds(ttl_secs),
            consumed: false,
        }
    }

    /// Consume the ticket, returning its token. Single use; expired
    /// tickets are rejected.
    pub fn consume(&mut self, now: DateTime<Utc>) -> Result<&Token, TerminalError> {
        if self.consumed {
            return Err(TerminalError::Used);
        }
        if now >= self.expires_at {
            return Err(TerminalError::Expired);
        }
        self.consumed = true;
        Ok(&self.token)
    }
}

/// Lifecycle state of a terminal session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    /// The PTY is running.
    Open,
    /// The session ended (idle timeout, overflow, client close).
    Closed,
}

/// Why a session closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CloseReason {
    /// The client disconnected.
    ClientClosed,
    /// No input/output within the idle window.
    IdleTimeout,
    /// Output outgrew the bounded buffer.
    Overflow,
}

impl CloseReason {
    /// Stable lower-case label used in records and audit metadata.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ClientClosed => "client_closed",
            Self::IdleTimeout => "idle_timeout",
            Self::Overflow => "overflow",
        }
    }
}

/// An audited terminal session record. Never contains stream content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminalSession {
    /// Session identifier.
    pub id: Uuid,
    /// Owning user.
    pub user_id: Uuid,
    /// Target site.
    pub site_id: Uuid,
    /// When the PTY was spawned.
    pub opened_at: DateTime<Utc>,
    /// When the session ended, if closed.
    pub closed_at: Option<DateTime<Utc>>,
    /// Current lifecycle state.
    pub state: SessionState,
    /// Why the session ended, if closed.
    pub close_reason: Option<CloseReason>,
}

impl TerminalSession {
    /// Open a session record at `opened_at`.
    pub fn open(user_id: Uuid, site_id: Uuid, opened_at: DateTime<Utc>) -> Self {
        Self {
            id: Uuid::new_v4(),
            user_id,
            site_id,
            opened_at,
            closed_at: None,
            state: SessionState::Open,
            close_reason: None,
        }
    }

    /// Close the session; idempotent — the first close wins.
    pub fn close(&mut self, reason: CloseReason, at: DateTime<Utc>) {
        if self.state == SessionState::Open {
            self.state = SessionState::Closed;
            self.closed_at = Some(at);
            self.close_reason = Some(reason);
        }
    }

    /// Session duration in seconds when closed.
    pub fn duration_secs(&self) -> Option<i64> {
        self.closed_at
            .map(|closed| (closed - self.opened_at).num_seconds())
    }
}

/// Specification for spawning a scoped PTY.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PtySpec {
    /// Unix user to run the shell as (the site's jailed user).
    pub user: String,
    /// Working directory inside the site root.
    pub cwd: String,
    /// Shell binary.
    pub shell: String,
}

/// One live PTY byte stream.
pub trait PtyStream: Send {
    /// Read output bytes (bounded by the caller's buffer).
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize>;
    /// Write input bytes from the client.
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize>;
    /// Poll whether the child has exited.
    fn has_exited(&mut self) -> std::io::Result<bool>;
}

/// Port that spawns a scoped PTY.
pub trait PtyPort: Send + Sync + 'static {
    /// Spawn a scoped PTY for the given specification.
    fn open(&self, spec: &PtySpec) -> Result<Box<dyn PtyStream>, TerminalError>;
}

/// Persistence port for tickets and sessions.
#[async_trait]
pub trait WebTerminalRepository: Send + Sync + 'static {
    /// Store a freshly minted ticket.
    async fn put_ticket(&self, ticket: &TerminalTicket) -> Result<(), RepoError>;
    /// Load a ticket by token.
    async fn take_ticket(&self, token: &str) -> Result<Option<TerminalTicket>, RepoError>;
    /// Persist updated (consumed) ticket state.
    async fn update_ticket(&self, ticket: &TerminalTicket) -> Result<(), RepoError>;
    /// Count open sessions for a user.
    async fn count_open_sessions(&self, user_id: Uuid) -> Result<u64, RepoError>;
    /// Insert an open-session record.
    async fn insert_session(&self, session: &TerminalSession) -> Result<(), RepoError>;
    /// Load one session record by id.
    async fn get_session(&self, id: Uuid) -> Result<Option<TerminalSession>, RepoError>;
    /// Close a session record.
    async fn close_session(
        &self,
        id: Uuid,
        reason: CloseReason,
        at: DateTime<Utc>,
    ) -> Result<(), RepoError>;
    /// Delete consumed/expired tickets older than `cutoff`.
    async fn prune_tickets(&self, cutoff: DateTime<Utc>) -> Result<u64, RepoError>;
}

#[cfg(test)]
mod tests;
