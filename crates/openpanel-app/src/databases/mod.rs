//! Databases bounded context: use cases + SQLite repository adapter + MySQL
//! CLI shell-out + AES-256-GCM password encryption.

pub mod crypto;
pub mod module;
pub mod mysql;
pub mod repo;
pub mod service;

pub use module::DatabasesModule;
pub use mysql::MySqlClient;
pub use service::DatabasesService;
