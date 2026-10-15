//! WebDAV bounded-context composition root.

use std::sync::Arc;

use openpanel_core::{AppContext, Migration, Module};
use openpanel_domain::{SiteRepository, webdav::WebDavRepository};

use super::{SqliteWebDavRepository, WebDavService, dav::router as dav_router};
use crate::sites::repo::SqliteSiteRepository;

/// Stable WebDAV module name.
pub const MODULE_NAME: &str = "webdav";

/// WebDAV composition root.
pub struct WebDavModule {
    service: Arc<WebDavService>,
    migrations: Vec<Migration>,
}

impl WebDavModule {
    /// Compose WebDAV persistence and lifecycle services.
    pub async fn new(ctx: &AppContext) -> Result<Self, openpanel_domain::webdav::WebDavError> {
        let pool = ctx.db.pool().await;
        let repo: Arc<dyn WebDavRepository> = Arc::new(SqliteWebDavRepository::new(pool.clone()));
        let sites: Arc<dyn SiteRepository> = Arc::new(SqliteSiteRepository::new(pool));
        let service = Arc::new(WebDavService::new(repo, sites, ctx.audit.clone()));
        Ok(Self {
            service,
            migrations: vec![Migration {
                module: MODULE_NAME,
                version: "001".into(),
                description: "per-site WebDAV accounts".into(),
                sql: crate::migrations::WEBDAV_V001.into(),
            }],
        })
    }

    /// Shared WebDAV lifecycle service.
    #[must_use]
    pub fn service(&self) -> Arc<WebDavService> {
        self.service.clone()
    }

    /// The `/dav` subtree router (mounted outside session auth).
    #[must_use]
    pub fn dav_router(&self) -> axum::Router {
        dav_router(self.service.clone())
    }
}

impl Module for WebDavModule {
    fn name(&self) -> &'static str {
        MODULE_NAME
    }

    fn migrations(&self) -> Vec<Migration> {
        self.migrations.clone()
    }
}
