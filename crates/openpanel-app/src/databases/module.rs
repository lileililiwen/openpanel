//! Databases module: registers the DatabasesService, migrations.

use std::sync::Arc;

use openpanel_core::{AppContext, Migration, Module};

use crate::databases::{
    mysql::MySqlClient, repo::SqliteDatabaseRepository, service::DatabasesService,
};

/// Stable identifier for the databases module used in migration bookkeeping.
pub const MODULE_NAME: &str = "databases";

/// Databases bounded-context module: wires the service + repository + MySQL client.
pub struct DatabasesModule {
    service: Arc<DatabasesService>,
    migrations: Vec<Migration>,
}

impl DatabasesModule {
    /// Build the module from the shared `AppContext` and the 32-byte master key
    /// used for AES-256-GCM password encryption.
    pub async fn new(ctx: &AppContext, master_key: [u8; 32]) -> Self {
        let pool = ctx.db.pool().await;
        let repo: Arc<dyn openpanel_domain::DatabaseRepository> =
            Arc::new(SqliteDatabaseRepository::new(pool));

        let mysql = MySqlClient::detect()
            .map(|p| MySqlClient::new(p, "root"))
            .unwrap_or_else(|| MySqlClient::new("/usr/bin/mysql", "root"));

        let service = Arc::new(DatabasesService::new(
            repo,
            mysql,
            ctx.audit.clone(),
            master_key,
        ));
        let migrations = vec![Migration {
            module: MODULE_NAME,
            version: "001".to_string(),
            description: "databases initial schema".to_string(),
            sql: crate::migrations::DATABASES_V001.to_string(),
        }];
        Self {
            service,
            migrations,
        }
    }

    /// Return a clone of the shared service handle.
    pub fn service(&self) -> Arc<DatabasesService> {
        self.service.clone()
    }
}

impl Module for DatabasesModule {
    fn name(&self) -> &'static str {
        MODULE_NAME
    }

    fn migrations(&self) -> Vec<Migration> {
        self.migrations.clone()
    }
}
