//! AI Ops application layer: SQLite repository, ask service, action
//! approval, and the tool executor. The bounded context is registered
//! via [`AiOpsModule`].

mod module;
mod repo;
mod service;
#[cfg(test)]
mod tests;

pub use module::{AiOpsModule, MODULE_NAME, default_allowlist};
pub use repo::SqliteAiOpsRepository;
pub use service::{
    ActionApproval, AskOutcome, AskService, DefaultToolExecutor, ToolExecutor, ToolExecutorError,
    ToolServices,
};
