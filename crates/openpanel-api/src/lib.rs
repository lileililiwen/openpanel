//! HTTP adapter: routes, middleware, DTOs, error mapping.

pub mod dto;
pub mod error;
pub mod extract;
pub mod middleware;
pub mod router;
pub mod routes;

pub use error::{ApiError, ApiResult};
pub use extract::{AuthUser, RequireOwner, RequireRole};
pub use router::build_router;