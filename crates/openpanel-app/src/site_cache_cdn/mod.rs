//! Site cache and CDN integration bounded context: cache policy
//! lifecycle, CDN integration management, nginx snippet generation,
//! and purge orchestration through registered adapters.

mod adapters;
mod module;
mod nginx_apply;
mod nginx_cache;
mod repo;
mod service;

pub use adapters::{
    CdnProviderConfig, CloudFrontAdapter, CloudflareAdapter, GenericHttpAdapter,
    provider_config_for,
};
pub use module::{MODULE_NAME, SiteCacheCdnModule};
pub use nginx_apply::{ApplyOutcome, NginxApplyError, NginxCacheManager};
pub use nginx_cache::{cache_directives, cache_path_directive, keys_zone_name};
pub use repo::SqliteSiteCacheCdnRepository;
pub use service::{CdnAdapterRegistry, PurgeSummary, SiteCacheService};
