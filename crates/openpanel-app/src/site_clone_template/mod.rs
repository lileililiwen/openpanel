//! Site clone and template export bounded context: the SQLite
//! repository, the `SiteCloneService` (live / snapshot / template
//! clone orchestration), the `TemplateExporter` (tar + Ed25519
//! signature + manifest), and the composition-root module.

mod module;
mod repo;
mod service;

pub use module::{MODULE_NAME, SiteCloneTemplateModule};
pub use repo::SqliteSiteCloneTemplateRepository;
pub use service::{
    FileEnumerator, FsEnumerator, SiteCloneService, TemplateArtifact, TemplateExporter,
    default_deny_patterns,
};
