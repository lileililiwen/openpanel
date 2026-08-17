//! WordPress toolkit composition module and migration registration.

use std::sync::Arc;

use openpanel_core::{AppContext, Migration, Module};

use super::{SqliteWpRepository, WpCacheLayer, WpScanner, WpToolkitService, WpUpdater};

/// Stable WordPress toolkit module name.
pub const MODULE_NAME: &str = "wordpress_toolkit";

/// WordPress toolkit bounded-context composition root.
pub struct WordPressToolkitModule {
    repo: Arc<SqliteWpRepository>,
    service: Arc<WpToolkitService>,
    migrations: Vec<Migration>,
}

impl WordPressToolkitModule {
    /// Compose the bounded context.
    pub async fn new(ctx: &AppContext) -> Self {
        let pool = ctx.db.pool().await;
        let repo = Arc::new(SqliteWpRepository::new(pool));
        let scanner = WpScanner::new(repo.clone(), ctx.audit.clone());
        let updater = WpUpdater::new(
            repo.clone(),
            Arc::new(super::FakeWpFilesystem::new(
                "/var/www/wp",
                vec!["wp-config.php".to_string()],
            )),
            ctx.audit.clone(),
        );
        let cache = WpCacheLayer::new(repo.clone(), ctx.audit.clone());
        let service = Arc::new(WpToolkitService::new(scanner, updater, cache));
        Self {
            repo,
            service,
            migrations: vec![Migration {
                module: MODULE_NAME,
                version: "001".to_owned(),
                description: "WordPress toolkit: managed sites and update runs".to_owned(),
                sql: crate::migrations::WORDPRESS_TOOLKIT_V001.to_owned(),
            }],
        }
    }

    /// Shared service.
    pub fn service(&self) -> Arc<WpToolkitService> {
        self.service.clone()
    }

    /// Shared repository.
    pub fn repo(&self) -> Arc<SqliteWpRepository> {
        self.repo.clone()
    }
}

impl Module for WordPressToolkitModule {
    fn name(&self) -> &'static str {
        MODULE_NAME
    }

    fn migrations(&self) -> Vec<Migration> {
        self.migrations.clone()
    }
}
