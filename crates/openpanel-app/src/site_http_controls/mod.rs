//! Site HTTP-controls bounded context: pure renderer, SQLite
//! repository, orchestration service, and composition module.

pub mod module;
mod renderer;
mod repo;
mod service;

pub use module::{MODULE_NAME, SiteHttpControlsModule};
pub use openpanel_domain::site_http_controls::{
    BasicAuthAccount, ClientIpRule, ErrorPageOverride, HotlinkPolicy, IndexPolicy, IpEffect,
    MimeOverride, ProtectedDir, RedirectRule, RedirectStatus, SiteHttpControls, SiteHttpError,
};
pub use renderer::SiteHttpControlsRenderer;
pub use repo::SqliteSiteHttpRepository;
pub use service::{NginxHttpControlsApplier, SiteHttpConfigApplier, SiteHttpService};
