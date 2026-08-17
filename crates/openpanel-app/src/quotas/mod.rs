//! Quotas application layer.

mod module;
mod repo;
mod service;

pub use module::QuotasModule;
pub use repo::SqliteQuotaRepository;
pub use service::QuotaService;
