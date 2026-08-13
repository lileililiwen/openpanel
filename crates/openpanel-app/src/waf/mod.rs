//! Per-site WAF application service, compiler, and adapters.

mod compiler;
mod module;
mod repo;
mod service;

pub use compiler::NginxSnippetCompiler;
pub use module::WafModule;
pub use repo::SqliteWafRepository;
pub use service::{DryRunResult, NginxWafConfigApplier, WafConfigApplier, WafService};
