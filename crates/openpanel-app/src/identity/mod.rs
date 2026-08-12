//! Identity bounded context: concrete services and repository adapters.

pub mod factor_repo;
pub mod module;
pub mod repo;
pub mod service;
pub mod two_factor;

pub use factor_repo::SqliteFactorRepository;
pub use module::IdentityModule;
pub use service::IdentityService;
pub use two_factor::TwoFactorService;
