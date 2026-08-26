//! Email deliverability application layer.

pub mod module;
mod service;

pub use module::{DeliverabilityModule, MODULE_NAME};
pub use service::DeliverabilityService;
