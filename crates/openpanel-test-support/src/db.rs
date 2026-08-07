//! Per-test SQLite database with RAII cleanup.
//!
//! `TestDb::new()` creates a fresh SQLite file under
//! `/tmp/openpanel-test/<uuid>.db`, connects, runs all migrations,
//! and returns the pool. `Drop` removes the file.

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use sqlx::{Pool, Sqlite};
use std::path::PathBuf;
use uuid::Uuid;

/// RAII wrapper around a per-test SQLite database file.
pub struct TestDb {
    path: PathBuf,
    pool: Pool<Sqlite>,
}

impl TestDb {
    /// Create a fresh test database. Panics if the connection or
    /// migration fails — those are programming errors, not test
    /// logic.
    pub async fn new() -> Self {
        let id = Uuid::new_v4();
        let dir = PathBuf::from("/tmp/openpanel-test");
        std::fs::create_dir_all(&dir).expect("create test dir");
        let path = dir.join(format!("{id}.db"));
        let url = format!("sqlite://{}?mode=rwc", path.display());

        let opts: SqliteConnectOptions = url.parse().expect("valid sqlite url");
        let opts = opts
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .busy_timeout(std::time::Duration::from_secs(5))
            .foreign_keys(true);

        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(opts)
            .await
            .expect("connect sqlite");

        let test_db = Self { path, pool };
        test_db.run_migrations().await;
        test_db
    }

    async fn run_migrations(&self) {
        sqlx::query(include_str!("migrations/000_audit.sql"))
            .execute(&self.pool)
            .await
            .expect("audit migration");
        sqlx::query(include_str!("migrations/identity/V001__init.sql"))
            .execute(&self.pool)
            .await
            .expect("identity migration");
        sqlx::query(include_str!("migrations/sites/V001__init.sql"))
            .execute(&self.pool)
            .await
            .expect("sites migration");
    }

    /// Access the underlying pool.
    pub fn pool(&self) -> Pool<Sqlite> {
        self.pool.clone()
    }

    /// The database file URL.
    pub fn url(&self) -> String {
        format!("sqlite://{}", self.path.display())
    }
}

impl Drop for TestDb {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
        let _ = std::fs::remove_file(format!("{}-wal", self.path.display()));
        let _ = std::fs::remove_file(format!("{}-shm", self.path.display()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn testdb_isolates_per_test() {
        let a = TestDb::new().await;
        let b = TestDb::new().await;
        assert_ne!(a.url(), b.url());
        // Dropping a should not affect b
        drop(a);
        let row = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM users")
            .fetch_one(&b.pool)
            .await
            .unwrap();
        assert_eq!(row, 0);
    }
}
