//! IPv6 + address-pool composition module and migration registration.

use std::sync::Arc;

use openpanel_core::{AppContext, Migration, Module};

use super::{Allocator, SqliteIpRepository, VhostBinder};

/// Stable IP allocation module name.
pub const MODULE_NAME: &str = "ip_allocation";

/// IPv6 + address-pool bounded-context composition root.
pub struct IpAllocationModule {
    repo: Arc<SqliteIpRepository>,
    allocator: Arc<Allocator>,
    binder: Arc<VhostBinder>,
    migrations: Vec<Migration>,
}

impl IpAllocationModule {
    /// Compose the bounded context.
    pub async fn new(ctx: &AppContext) -> Self {
        let pool = ctx.db.pool().await;
        let repo = Arc::new(SqliteIpRepository::new(pool));
        let allocator = Arc::new(Allocator::new(repo.clone(), ctx.audit.clone()));
        let binder = Arc::new(VhostBinder::new(repo.clone(), ctx.audit.clone()));
        Self {
            repo,
            allocator,
            binder,
            migrations: vec![Migration {
                module: MODULE_NAME,
                version: "001".to_owned(),
                description: "IPv6 + address-pool bounded context".to_owned(),
                sql: crate::migrations::IP_ALLOCATION_V001.to_owned(),
            }],
        }
    }

    /// Shared allocator.
    pub fn allocator(&self) -> Arc<Allocator> {
        self.allocator.clone()
    }

    /// Shared vhost binder.
    pub fn binder(&self) -> Arc<VhostBinder> {
        self.binder.clone()
    }

    /// Shared repository.
    pub fn repo(&self) -> Arc<SqliteIpRepository> {
        self.repo.clone()
    }
}

impl Module for IpAllocationModule {
    fn name(&self) -> &'static str {
        MODULE_NAME
    }

    fn migrations(&self) -> Vec<Migration> {
        self.migrations.clone()
    }
}
