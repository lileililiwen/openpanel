//! Application layer: use-case services and SQLite repository adapters.
//! Concrete modules (IdentityModule, etc.) live here. Each implements the
//! `Module` trait from `openpanel-core` and registers its services,
//! routes, CLI, and migrations.

pub mod identity;
pub mod migrations;
pub mod prelude;
pub mod sites;

pub use identity::IdentityModule;
pub use identity::service::IdentityService;
pub use sites::SitesModule;
pub use sites::nginx::{NginxConfigGenerator, NginxPaths};
pub use sites::service::SitesService;