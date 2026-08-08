//! Identity module: registers the identity service, HTTP routes, CLI
//! commands, and migrations.

use std::sync::Arc;

use openpanel_core::{AppContext, Migration, Module};

use crate::identity::service::IdentityService;

/// Stable identifier for the identity module used in migration bookkeeping.
pub const MODULE_NAME: &str = "identity";

/// Identity bounded-context module: wires the service + SQLite repos + migrations.
pub struct IdentityModule {
    service: Arc<IdentityService>,
    migrations: Vec<Migration>,
}

impl IdentityModule {
    /// Build the module from an `AppContext`. The caller must have already
    /// registered the SQLite repositories and audit service.
    pub async fn new(ctx: &AppContext) -> Self {
        let pool = ctx.db.pool().await;
        let (users, sessions) = crate::identity::service::build_repos(pool);
        let service = Arc::new(IdentityService::new(users, sessions, ctx.audit.clone()));
        let migrations = vec![Migration {
            module: MODULE_NAME,
            version: "001".to_string(),
            description: "identity initial schema".to_string(),
            sql: crate::migrations::IDENTITY_V001.to_string(),
        }];
        Self {
            service,
            migrations,
        }
    }

    /// Return a clone of the shared service handle.
    pub fn service(&self) -> Arc<IdentityService> {
        self.service.clone()
    }
}

impl Module for IdentityModule {
    fn name(&self) -> &'static str {
        MODULE_NAME
    }

    fn migrations(&self) -> Vec<Migration> {
        self.migrations.clone()
    }
}
