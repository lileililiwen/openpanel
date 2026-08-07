//! Identity bounded context: concrete services and repository adapters.

pub mod module;
pub mod repo;
pub mod service;

pub use module::IdentityModule;
pub use service::IdentityService;
