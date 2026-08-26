//! Authorized log browsing and traffic-insight use cases.

mod module;
pub mod rotation_service;
mod service;
mod task;

pub use module::*;
pub use rotation_service::{LogRotationService, detect_logrotate};
pub use service::*;
