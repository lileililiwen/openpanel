//! Web-terminal bounded context: SQLite repository, PTY adapter,
//! orchestration service, and composition module.

pub mod module;
mod pty;
mod repo;
mod service;

pub use module::{MODULE_NAME, WebTerminalModule};
pub use openpanel_domain::web_terminal::{
    CloseReason, PtyPort, PtySpec, PtyStream, SessionState, TerminalError, TerminalSession,
    TerminalTicket, Token,
};
pub use pty::PortablePtyAdapter;
pub use repo::SqliteWebTerminalRepository;
pub use service::{TerminalPolicy, WebTerminalService};
