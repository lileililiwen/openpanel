//! HTTP adapter: routes, middleware, DTOs, error mapping.

#![deny(rustdoc::broken_intra_doc_links)]

/// Data-transfer objects (request/response bodies) for the HTTP API.
pub mod dto;
/// HTTP error type and its mapping to JSON responses.
pub mod error;
/// Axum request extractors (`AuthUser`, `RequireRole`, `RequireOwner`).
pub mod extract;
/// Reusable HTTP middleware (currently session resolution).
pub mod middleware;
/// Top-level router builder composing every route module.
pub mod router;
/// Per-resource HTTP route modules (identity, sites, databases, files).
pub mod routes;

pub use error::{ApiError, ApiResult};
pub use extract::{AuthUser, RequireOwner, RequireRole};
pub use router::build_router;
