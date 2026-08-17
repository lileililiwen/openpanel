//! Hosting-plans application layer: validation, lifecycle, and the
//! read-side resolver consumed by other bounded contexts.

mod module;
mod repo;
mod service;

pub use module::HostingPlansModule;
pub use repo::SqliteHostingPlanRepository;
pub use service::HostingPlansService;
