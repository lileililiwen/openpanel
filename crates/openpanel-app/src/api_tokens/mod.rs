//! Scoped personal API-token application and persistence adapters.

mod module;
mod repo;
mod service;

pub use module::ApiTokenModule;
pub use repo::SqliteApiTokenRepository;
pub use service::{
    ApiTokenService, BearerPrincipal, CreateApiToken, CreatedApiToken, RateLimitConfig,
    TokenAuthError,
};
