//! Identity module: registers the identity service, HTTP routes, CLI
//! commands, and migrations.

use std::sync::Arc;

use openpanel_core::{AppContext, Migration, Module};

use crate::identity::{
    factor_repo::SqliteFactorRepository, service::IdentityService, two_factor::TwoFactorService,
};

/// Stable identifier for the identity module used in migration bookkeeping.
pub const MODULE_NAME: &str = "identity";

/// Identity bounded-context module: wires the service + SQLite repos + migrations.
pub struct IdentityModule {
    service: Arc<IdentityService>,
    two_factor: Arc<TwoFactorService>,
    migrations: Vec<Migration>,
}

impl IdentityModule {
    /// Build the module from an `AppContext`. The caller must provide
    /// the panel master key (used to encrypt TOTP secrets at rest).
    pub async fn new(ctx: &AppContext, master_key: [u8; 32]) -> Self {
        let pool = ctx.db.pool().await;
        let (users, sessions) = crate::identity::service::build_repos(pool.clone());
        let factors = Arc::new(SqliteFactorRepository::new(pool));
        let two_factor = Arc::new(TwoFactorService::new(
            factors,
            master_key,
            ctx.audit.clone(),
        ));
        let service = Arc::new(IdentityService::new(
            users,
            sessions,
            ctx.audit.clone(),
            two_factor.clone(),
        ));
        let migrations = vec![
            Migration {
                module: MODULE_NAME,
                version: "001".to_string(),
                description: "identity initial schema".to_string(),
                sql: crate::migrations::IDENTITY_V001.to_string(),
            },
            Migration {
                module: MODULE_NAME,
                version: "002".to_string(),
                description: "two-factor authentication (TOTP, recovery codes, login challenges)"
                    .to_string(),
                sql: crate::migrations::IDENTITY_V002.to_string(),
            },
            Migration {
                module: MODULE_NAME,
                version: "003".to_string(),
                description: "WebAuthn credentials and ceremony challenges".to_string(),
                sql: crate::migrations::IDENTITY_V003.to_string(),
            },
        ];
        Self {
            service,
            two_factor,
            migrations,
        }
    }

    /// Return a clone of the shared identity service handle.
    pub fn service(&self) -> Arc<IdentityService> {
        self.service.clone()
    }

    /// Return a clone of the shared two-factor service handle.
    pub fn two_factor(&self) -> Arc<TwoFactorService> {
        self.two_factor.clone()
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
