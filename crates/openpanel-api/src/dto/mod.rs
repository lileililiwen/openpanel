/// Authentication- and user-related request/response DTOs.
pub mod auth;
/// Shared DTOs used by multiple endpoints (e.g. generic error envelope).
pub mod common;
/// Database management request/response DTOs.
pub mod database;
/// File-manager request/response DTOs.
pub mod file;
/// Monitoring request/response DTOs.
pub mod monitoring;
/// Site management request/response DTOs.
pub mod site;
/// SSL management request/response DTOs.
pub mod ssl;

pub use auth::*;
pub use common::*;
pub use database::*;
pub use file::*;
pub use monitoring::*;
pub use site::*;
pub use ssl::*;
