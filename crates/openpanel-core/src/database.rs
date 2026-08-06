//! Database abstraction. v0.1 ships only the SQLite driver; the trait leaves
//! room for a Postgres driver later without touching modules.

use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use sqlx::{Pool, Sqlite};
use tokio::sync::RwLock;

use crate::error::{CoreResult, DatabaseError};

pub type SqlitePool = Pool<Sqlite>;

/// Database contract used by application services. v0.1 only exposes
/// SQLite-specific queries, but the trait lets future drivers swap in.
#[async_trait]
pub trait DatabaseDriver: Send + Sync + 'static {
    /// Return an owned clone of the pool. Cheap — `Pool` is `Arc` internally.
    async fn pool(&self) -> SqlitePool;

    async fn begin(&self) -> Result<sqlx::Transaction<'_, Sqlite>, DatabaseError>;
}

/// SQLite-backed driver. Enables WAL and a busy timeout by default.
pub struct SqliteDriver {
    url: String,
    pool: RwLock<Option<SqlitePool>>,
}

impl SqliteDriver {
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            pool: RwLock::new(None),
        }
    }

    pub async fn connect(&self) -> CoreResult<SqlitePool> {
        let opts: SqliteConnectOptions = self
            .url
            .parse()
            .map_err(DatabaseError::Sqlx)?;
        let opts = opts
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .busy_timeout(std::time::Duration::from_secs(5))
            .foreign_keys(true);

        if let Some(stripped) = self.url.strip_prefix("sqlite://").or(self.url.strip_prefix("sqlite:")) {
            if let Some(parent) = Path::new(stripped).parent() {
                if !parent.as_os_str().is_empty() && !parent.exists() {
                    std::fs::create_dir_all(parent)?;
                }
            }
        }

        let pool = SqlitePoolOptions::new()
            .max_connections(8)
            .connect_with(opts)
            .await
            .map_err(DatabaseError::Sqlx)?;

        let mut guard = self.pool.write().await;
        *guard = Some(pool.clone());
        Ok(pool)
    }
}

impl SqliteDriver {
    pub fn shared(self) -> Arc<Self> {
        Arc::new(self)
    }
}

#[async_trait]
impl DatabaseDriver for SqliteDriver {
    async fn pool(&self) -> SqlitePool {
        let guard = self.pool.read().await;
        guard
            .clone()
            .expect("SqliteDriver::connect must be awaited before pool()")
    }

    async fn begin(&self) -> Result<sqlx::Transaction<'_, Sqlite>, DatabaseError> {
        let pool = self.pool().await;
        pool.begin().await.map_err(DatabaseError::Sqlx)
    }
}