//! Application layer: use-case services and SQLite repository adapters.
//! Concrete modules (IdentityModule, etc.) live here. Each implements the
//! `Module` trait from `openpanel-core` and registers its services,
//! routes, CLI, and migrations.

#![deny(rustdoc::broken_intra_doc_links)]

pub mod databases;
pub mod files;
pub mod identity;
pub mod migrations;
pub mod prelude;
pub mod sites;

pub use databases::DatabasesModule;
pub use databases::service::DatabasesService;
pub use files::FilesModule;
pub use files::service::FilesService;
pub use identity::IdentityModule;
pub use identity::service::IdentityService;
pub use sites::SitesModule;
pub use sites::nginx::{NginxConfigGenerator, NginxPaths};
pub use sites::service::SitesService;
