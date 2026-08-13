//! FTP bounded-context composition root.

use std::sync::Arc;

use openpanel_core::{AppContext, Migration, Module};
use openpanel_domain::{SiteRepository, ftp::FtpRepository};

use super::{
    FtpAuthenticator, FtpServerConfig, FtpServerTask, FtpService, FtpSessionRegistry,
    SqliteFtpRepository,
};
use crate::sites::repo::SqliteSiteRepository;

/// Stable FTP module name.
pub const MODULE_NAME: &str = "ftp";

/// FTP composition root.
pub struct FtpModule {
    service: Arc<FtpService>,
    migrations: Vec<Migration>,
    config: FtpServerConfig,
    repo: Arc<dyn FtpRepository>,
    sites: Arc<dyn SiteRepository>,
    sessions: Arc<FtpSessionRegistry>,
    audit: Arc<dyn openpanel_core::AuditService>,
}

impl FtpModule {
    /// Compose FTP persistence and lifecycle services.
    pub async fn new(ctx: &AppContext) -> Result<Self, openpanel_domain::ftp::FtpError> {
        let pool = ctx.db.pool().await;
        let repo: Arc<dyn FtpRepository> = Arc::new(SqliteFtpRepository::new(pool.clone()));
        let sites: Arc<dyn SiteRepository> = Arc::new(SqliteSiteRepository::new(pool));
        let sessions = Arc::new(FtpSessionRegistry::default());
        let service = Arc::new(FtpService::new(
            repo.clone(),
            sites.clone(),
            ctx.audit.clone(),
            sessions.clone(),
        ));
        Ok(Self {
            service,
            migrations: vec![Migration {
                module: MODULE_NAME,
                version: "001".into(),
                description: "per-site FTP accounts".into(),
                sql: crate::migrations::FTP_V001.into(),
            }],
            config: FtpServerConfig::from_env()?,
            repo,
            sites,
            sessions,
            audit: ctx.audit.clone(),
        })
    }

    /// Shared FTP lifecycle service.
    pub fn service(&self) -> Arc<FtpService> {
        self.service.clone()
    }
}

impl Module for FtpModule {
    fn name(&self) -> &'static str {
        MODULE_NAME
    }

    fn migrations(&self) -> Vec<Migration> {
        self.migrations.clone()
    }

    fn background_tasks(
        &self,
        _ctx: &AppContext,
    ) -> Vec<Box<dyn openpanel_core::jobs::BackgroundTask>> {
        if !self.config.enabled {
            return Vec::new();
        }
        let auth = Arc::new(FtpAuthenticator::new(
            self.repo.clone(),
            self.sites.clone(),
            self.audit.clone(),
            self.sessions.clone(),
        ));
        vec![Box::new(FtpServerTask::new(
            self.config.clone(),
            self.repo.clone(),
            auth,
            self.sessions.clone(),
            self.audit.clone(),
        ))]
    }
}
