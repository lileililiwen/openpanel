//! Staging mail-capture module composition root.

use std::sync::Arc;

use openpanel_core::{AppContext, Migration, Module};
use openpanel_domain::SiteRepository;

use super::service::MailCaptureService;
use crate::databases::crypto::KEY_LEN;
use crate::sites::repo::SqliteSiteRepository;

/// Stable mail-capture module name.
pub const MODULE_NAME: &str = "mail-capture";

/// Mail-capture composition root.
pub struct MailCaptureModule {
    service: Arc<MailCaptureService>,
    migrations: Vec<Migration>,
}

impl MailCaptureModule {
    /// Compose the capture inbox service. The migration set assumes
    /// `site-staging` (including its V002 capture toggle) is applied
    /// first: the message table has a FK onto `staging_slots`.
    pub async fn new(ctx: &AppContext, master_key: [u8; KEY_LEN]) -> Self {
        let pool = ctx.db.pool().await;
        let sites: Arc<dyn SiteRepository> = Arc::new(SqliteSiteRepository::new(pool.clone()));
        let service = Arc::new(MailCaptureService::new(
            pool,
            sites,
            master_key,
            ctx.audit.clone(),
        ));
        Self {
            service,
            migrations: vec![Migration {
                module: MODULE_NAME,
                version: "001".into(),
                description: "staging mail-capture inbox".into(),
                sql: crate::migrations::MAIL_CAPTURE_V001.into(),
            }],
        }
    }

    /// Shared capture service handle.
    #[must_use]
    pub fn service(&self) -> Arc<MailCaptureService> {
        self.service.clone()
    }
}

impl Module for MailCaptureModule {
    fn name(&self) -> &'static str {
        MODULE_NAME
    }

    fn migrations(&self) -> Vec<Migration> {
        self.migrations.clone()
    }
}
