//! SSO composition module and migration registration.

use std::sync::Arc;

use openpanel_core::{AppContext, Migration, Module};
use openpanel_domain::{SsoRepository, UserRepository};

use super::{OpenidConnectAdapter, SqliteSsoRepository, SsoService};
use crate::identity::repo::{SqliteSessionRepository, SqliteUserRepository};

/// Stable module name.
pub const MODULE_NAME: &str = "identity_sso";

/// SSO bounded-context composition root.
pub struct SsoModule {
    service: Arc<SsoService>,
    migrations: Vec<Migration>,
}

impl SsoModule {
    /// Compose SQLite persistence and the reqwest OIDC adapter.
    pub async fn new(ctx: &AppContext, master_key: [u8; 32]) -> Self {
        let pool = ctx.db.pool().await;
        let repo: Arc<dyn SsoRepository> = Arc::new(SqliteSsoRepository::new(pool.clone()));
        let users: Arc<dyn UserRepository> = Arc::new(SqliteUserRepository::new(pool.clone()));
        let sessions: Arc<dyn openpanel_domain::SessionRepository> =
            Arc::new(SqliteSessionRepository::new(pool));
        let key = std::sync::Arc::new(master_key);
        let resolver_key = std::sync::Arc::clone(&key);
        let oidc = Arc::new(OpenidConnectAdapter::new(
            "/auth/sso/callback".to_owned(),
            Arc::new(move |cipher: &str| {
                crate::databases::crypto::decrypt_from_storage(&resolver_key, cipher)
                    .map_err(|error| openpanel_domain::SsoError::InvalidConfig(error.to_string()))
            }),
        ));
        let service = Arc::new(SsoService::new(
            repo,
            users,
            sessions,
            ctx.audit.clone(),
            oidc,
            *key,
        ));
        Self {
            service,
            migrations: vec![Migration {
                module: MODULE_NAME,
                version: "001".to_owned(),
                description: "sso connections, identity links, login states".to_owned(),
                sql: include_str!("V001__init.sql").to_owned(),
            }],
        }
    }

    /// Shared service.
    pub fn service(&self) -> Arc<SsoService> {
        self.service.clone()
    }
}

impl Module for SsoModule {
    fn name(&self) -> &'static str {
        MODULE_NAME
    }

    fn migrations(&self) -> Vec<Migration> {
        self.migrations.clone()
    }
}
