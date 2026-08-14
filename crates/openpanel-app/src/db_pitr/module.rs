//! `db-pitr` module composition: wires the service, repositories, and
//! migrations into the shared [`ModuleRegistry`](openpanel_core::Module).

use std::sync::Arc;

use openpanel_core::{AppContext, AuditService, Migration, Module};
use openpanel_domain::{
    BinlogSink, BinlogStreamRepository, DatabaseLookup, IncrementalRepository, LogTailer,
    PitrRepository,
};

use crate::db_pitr::{
    repo::{SqliteBinlogStreamRepository, SqliteIncrementalRepository, SqlitePitrRepository},
    service::PitrService,
};

/// Stable module identifier.
pub const MODULE_NAME: &str = "db-pitr";

/// Database point-in-time recovery module: binlog streaming, PITR
/// restore, and incremental file-backup deltas.
pub struct DbPitrModule {
    service: Arc<PitrService>,
    migrations: Vec<Migration>,
}

impl DbPitrModule {
    /// Build the module from the shared `AppContext`, a sink, and a
    /// tailer. The repositories are built from the app database pool.
    #[allow(clippy::too_many_arguments)]
    pub async fn new(
        ctx: &AppContext,
        sink: Arc<dyn BinlogSink>,
        tailers: Vec<Arc<dyn LogTailer>>,
        databases: Arc<dyn DatabaseLookup>,
        audit: Arc<dyn AuditService>,
    ) -> Self {
        let pool = ctx.db.pool().await;
        let streams: Arc<dyn BinlogStreamRepository> =
            Arc::new(SqliteBinlogStreamRepository::new(pool.clone()));
        let restores: Arc<dyn PitrRepository> = Arc::new(SqlitePitrRepository::new(pool.clone()));
        let incrementals: Arc<dyn IncrementalRepository> =
            Arc::new(SqliteIncrementalRepository::new(pool.clone()));
        let service = Arc::new(PitrService::new(
            streams,
            restores,
            incrementals,
            databases,
            sink,
            tailers,
            audit,
        ));
        Self {
            service,
            migrations: vec![Migration {
                module: MODULE_NAME.into(),
                version: "001".into(),
                description: "db-pitr initial schema".into(),
                sql: crate::migrations::DB_PITR_V001.into(),
            }],
        }
    }

    /// Shared service handle.
    pub fn service(&self) -> Arc<PitrService> {
        self.service.clone()
    }
}

impl Module for DbPitrModule {
    fn name(&self) -> &'static str {
        MODULE_NAME
    }

    fn migrations(&self) -> Vec<Migration> {
        self.migrations.clone()
    }
}
