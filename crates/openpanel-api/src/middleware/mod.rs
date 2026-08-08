//! Reusable Axum middleware modules.

/// Session resolution middleware (bearer / cookie → `(User, Session)`).
pub mod session;
