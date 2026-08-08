//! Files module: registers the FilesService. No migrations.

use std::sync::Arc;

use openpanel_core::{AppContext, Module};
use openpanel_domain::SiteRepository;

use crate::{files::service::FilesService, sites::repo::SqliteSiteRepository};

/// Stable identifier for the files module used in module bookkeeping.
pub const MODULE_NAME: &str = "files";

/// Files bounded-context module: wires the `FilesService` + filesystem repo.
pub struct FilesModule {
    service: Arc<FilesService>,
}

impl FilesModule {
    /// Build the module from the shared `AppContext`.
    pub async fn new(ctx: &AppContext) -> Self {
        let pool = ctx.db.pool().await;
        let sites_repo: Arc<dyn SiteRepository> = Arc::new(SqliteSiteRepository::new(pool));
        let service = Arc::new(FilesService::new(sites_repo, ctx.audit.clone()));
        Self { service }
    }

    /// Return a clone of the shared service handle.
    pub fn service(&self) -> Arc<FilesService> {
        self.service.clone()
    }
}

impl Module for FilesModule {
    fn name(&self) -> &'static str {
        MODULE_NAME
    }
}
