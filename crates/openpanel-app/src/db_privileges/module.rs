//! Database privilege management composition module and migration registration.

use std::sync::Arc;

use openpanel_core::{AppContext, Migration, Module};

use super::{
    AdminToolSso, PrivilegeService, RemoteAccessController, SqliteDbPrivilegeRepository,
};

/// Stable DB privilege module name.
pub const MODULE_NAME: &str = "db_privileges";

/// Database privilege management bounded-context composition root.
pub struct DbPrivilegeModule {
    repo: Arc<SqliteDbPrivilegeRepository>,
    privilege: Arc<PrivilegeService>,
    remote: Arc<RemoteAccessController>,
    sso: Arc<AdminToolSso>,
    migrations: Vec<Migration>,
}

impl DbPrivilegeModule {
    /// Compose the bounded context.
    pub async fn new(ctx: &AppContext) -> Self {
        let pool = ctx.db.pool().await;
        let repo = Arc::new(SqliteDbPrivilegeRepository::new(pool));
        let privilege = Arc::new(PrivilegeService::new(
            repo.clone(),
            ctx.audit.clone(),
        ));
        let remote = Arc::new(RemoteAccessController::new(
            repo.clone(),
            ctx.audit.clone(),
        ));
        let sso = Arc::new(AdminToolSso::new(repo.clone(), ctx.audit.clone()));
        Self {
            repo,
            privilege,
            remote,
            sso,
            migrations: vec![Migration {
                module: MODULE_NAME,
                version: "001".to_owned(),
                description: "db grants, remote access, admin-tool SSO".to_owned(),
                sql: crate::migrations::DB_PRIVILEGES_V001.to_owned(),
            }],
        }
    }

    /// Shared privilege service.
    pub fn privilege(&self) -> Arc<PrivilegeService> {
        self.privilege.clone()
    }

    /// Shared remote access controller.
    pub fn remote(&self) -> Arc<RemoteAccessController> {
        self.remote.clone()
    }

    /// Shared SSO issuer.
    pub fn sso(&self) -> Arc<AdminToolSso> {
        self.sso.clone()
    }

    /// Shared repository.
    pub fn repo(&self) -> Arc<SqliteDbPrivilegeRepository> {
        self.repo.clone()
    }
}

impl Module for DbPrivilegeModule {
    fn name(&self) -> &'static str {
        MODULE_NAME
    }

    fn migrations(&self) -> Vec<Migration> {
        self.migrations.clone()
    }
}
