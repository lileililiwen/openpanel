//! Non-PHP runtime application layer: SQLite repository, runtime
//! service, supervisor unit builder, reverse-proxy layer, and
//! module composition.

pub mod env_service;
mod module;
mod repo;
mod service;
#[cfg(test)]
mod tests;

pub use env_service::RuntimeEnvService;
pub use module::{AppRuntimesModule, MODULE_NAME};
pub use repo::SqliteRuntimeRepository;
pub use service::{
    ReverseProxyLayer, RuntimeService, SupervisorUnitBuilder, render_proxy_block, render_unit,
};
