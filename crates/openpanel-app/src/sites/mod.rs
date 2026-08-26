//! Sites bounded context: use cases + SQLite repository adapter + nginx
//! config generator + document root provisioning.

pub mod document_root;
pub mod module;
pub mod nginx;
pub mod repo;
pub mod service;
pub mod transport_service;

pub use module::SitesModule;
pub use nginx::{NginxConfigGenerator, NginxPaths};
pub use service::SitesService;
pub use transport_service::SiteTransportService;
