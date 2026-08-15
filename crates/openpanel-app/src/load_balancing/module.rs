//! Load balancing and failover composition module and migration registration.

use std::sync::Arc;

use openpanel_core::{AppContext, Migration, Module};

use super::{LbService, MemberRotator, SqliteLbRepository};

/// Stable load-balancing module name.
pub const MODULE_NAME: &str = "load_balancing";

/// Load balancing and failover bounded-context composition root.
pub struct LoadBalancingModule {
    repo: Arc<SqliteLbRepository>,
    service: Arc<LbService>,
    migrations: Vec<Migration>,
}

impl LoadBalancingModule {
    /// Compose the bounded context.
    pub async fn new(ctx: &AppContext) -> Self {
        let pool = ctx.db.pool().await;
        let repo = Arc::new(SqliteLbRepository::new(pool));
        let rotator = MemberRotator::new(repo.clone(), ctx.audit.clone());
        let service = Arc::new(LbService::new(repo.clone(), rotator));
        Self {
            repo,
            service,
            migrations: vec![Migration {
                module: MODULE_NAME,
                version: "001".to_owned(),
                description: "load balancing: pools and members".to_owned(),
                sql: crate::migrations::LOAD_BALANCING_V001.to_owned(),
            }],
        }
    }

    /// Shared service.
    pub fn service(&self) -> Arc<LbService> {
        self.service.clone()
    }

    /// Shared repository.
    pub fn repo(&self) -> Arc<SqliteLbRepository> {
        self.repo.clone()
    }
}

impl Module for LoadBalancingModule {
    fn name(&self) -> &'static str {
        MODULE_NAME
    }

    fn migrations(&self) -> Vec<Migration> {
        self.migrations.clone()
    }
}
