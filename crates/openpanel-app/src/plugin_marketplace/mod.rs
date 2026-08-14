//! Plugin marketplace bounded context: remote catalog discovery,
//! publisher CA verification, and rating/metadata cache.
//!
//! The marketplace layer sits on top of `openpanel-app::plugin`. It
//! owns:
//!
//! - The HTTP transport that fetches a signed envelope from the
//!   remote catalog endpoint.
//! - The SQLite-backed `CatalogCache` adapter.
//! - The `MarketplaceService` that orchestrates discovery,
//!   signature verification, and delegated install.

pub mod cache;
pub mod client;
pub mod module;
pub mod service;
#[cfg(test)]
mod service_tests;

pub use cache::SqliteCatalogCache;
pub use client::{HttpMarketplaceClient, MarketplaceClient, MockMarketplaceClient};
pub use module::{MODULE_NAME, PluginMarketplaceModule};
pub use service::{
    DiscoverOutcome, InstallFromMarketplaceError, InstallFromMarketplaceRequest,
    MarketplaceService,
};