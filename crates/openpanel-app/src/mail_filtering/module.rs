//! Mail anti-spam and filtering composition module and migration registration.

use std::sync::Arc;

use openpanel_core::{AppContext, Migration, Module};

use super::{MailFilterService, MailingListService, SieveCompiler, SqliteMailFilterRepository};

/// Stable mail-filtering module name.
pub const MODULE_NAME: &str = "mail_filtering";

/// Mail anti-spam and filtering bounded-context composition root.
pub struct MailFilteringModule {
    repo: Arc<SqliteMailFilterRepository>,
    service: Arc<MailFilterService>,
    mailing_lists: Arc<MailingListService>,
    migrations: Vec<Migration>,
}

impl MailFilteringModule {
    /// Compose the bounded context.
    pub async fn new(ctx: &AppContext) -> Self {
        let pool = ctx.db.pool().await;
        let repo = Arc::new(SqliteMailFilterRepository::new(pool));
        let service = Arc::new(MailFilterService::new(
            repo.clone(),
            ctx.audit.clone(),
            SieveCompiler::new(),
        ));
        let mailing_lists = Arc::new(MailingListService::new(repo.clone()));
        Self {
            repo,
            service,
            mailing_lists,
            migrations: vec![Migration {
                module: MODULE_NAME,
                version: "001".to_owned(),
                description: "mail anti-spam + filtering + lists".to_owned(),
                sql: crate::migrations::MAIL_FILTERING_V001.to_owned(),
            }],
        }
    }

    /// Shared mail filter service.
    pub fn service(&self) -> Arc<MailFilterService> {
        self.service.clone()
    }

    /// Shared mailing list service.
    pub fn mailing_lists(&self) -> Arc<MailingListService> {
        self.mailing_lists.clone()
    }

    /// Shared repository.
    pub fn repo(&self) -> Arc<SqliteMailFilterRepository> {
        self.repo.clone()
    }
}

impl Module for MailFilteringModule {
    fn name(&self) -> &'static str {
        MODULE_NAME
    }

    fn migrations(&self) -> Vec<Migration> {
        self.migrations.clone()
    }
}