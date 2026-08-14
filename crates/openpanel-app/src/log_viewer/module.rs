//! Log viewer composition module and migration registration.

use std::sync::Arc;

use openpanel_core::{AppContext, Migration, Module};

use super::{InMemoryLogReader, LogAggregator, SqliteLogViewerRepository};

/// Stable log viewer module name.
pub const MODULE_NAME: &str = "log_viewer";

/// Log viewer bounded-context composition root.
pub struct LogViewerModule {
    repo: Arc<SqliteLogViewerRepository>,
    reader: Arc<InMemoryLogReader>,
    aggregator: Arc<LogAggregator>,
    migrations: Vec<Migration>,
}

impl LogViewerModule {
    /// Compose the bounded context with the default in-memory
    /// reader. Production wiring swaps in a JSONL-backed
    /// `LogReader`.
    pub async fn new(ctx: &AppContext) -> Self {
        let pool = ctx.db.pool().await;
        let repo = Arc::new(SqliteLogViewerRepository::new(pool));
        let reader = Arc::new(InMemoryLogReader::new());
        let aggregator = Arc::new(LogAggregator::new(
            reader.clone(),
            repo.clone(),
            ctx.audit.clone(),
        ));
        Self {
            repo,
            reader,
            aggregator,
            migrations: vec![Migration {
                module: MODULE_NAME,
                version: "001".to_owned(),
                description: "log viewer download audit rows".to_owned(),
                sql: crate::migrations::LOG_VIEWER_V001.to_owned(),
            }],
        }
    }

    /// Shared aggregator.
    pub fn aggregator(&self) -> Arc<LogAggregator> {
        self.aggregator.clone()
    }

    /// Shared reader (in-memory).
    pub fn reader(&self) -> Arc<InMemoryLogReader> {
        self.reader.clone()
    }

    /// Shared repository.
    pub fn repo(&self) -> Arc<SqliteLogViewerRepository> {
        self.repo.clone()
    }
}

impl Module for LogViewerModule {
    fn name(&self) -> &'static str {
        MODULE_NAME
    }

    fn migrations(&self) -> Vec<Migration> {
        self.migrations.clone()
    }
}
