//! Marketplace domain layer.
//!
//! This module is the discovery / catalog layer that sits on top of
//! `openpanel-domain::plugin`. It carries the catalog data model
//! (plugin entries, ratings, summary), the publisher signature
//! contract (which keys are accepted by the marketplace CA), and
//! the abstract transport / cache traits. I/O lives in
//! `openpanel-app`.

pub mod cache;
pub mod catalog;
pub mod error;
pub mod publisher;

pub use cache::{CatalogCache, CatalogSnapshot};
pub use catalog::{
    MarketplaceCatalog, MarketplacePlugin, PluginRating, SignedCatalogEnvelope, verify_envelope,
};
pub use error::PluginMarketplaceError;
pub use publisher::{MarketplaceCa, PublisherSignature};
