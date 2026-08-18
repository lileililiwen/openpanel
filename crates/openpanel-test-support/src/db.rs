//! Per-test SQLite database with RAII cleanup.
//!
//! `TestDb::new()` creates a fresh SQLite file under
//! `/tmp/openpanel-test/<uuid>.db`, connects, runs all migrations,
//! and returns the pool. `Drop` removes the file.

use std::path::PathBuf;

use sqlx::{
    Pool, Sqlite,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions},
};
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
    ///
    /// Every test DB switches the domain's argon2 / bcrypt costs to their
    /// fast test-fixture values so user-creation and recovery-code flows
    /// don't spend ~0.5s per hash. Production binaries never construct a
    /// `TestDb`, so the production defaults are untouched.
    pub async fn new() -> Self {
        openpanel_domain::Password::set_test_costs(8, 1);
        openpanel_domain::identity::RecoveryCode::set_test_cost(4);
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
        sqlx::query(include_str!("migrations/databases/V001__init.sql"))
            .execute(&self.pool)
            .await
            .expect("databases migration");
        sqlx::query(include_str!("migrations/db_pitr/V001__init.sql"))
            .execute(&self.pool)
            .await
            .expect("db_pitr migration");
        sqlx::query(include_str!("migrations/site_staging/V001__init.sql"))
            .execute(&self.pool)
            .await
            .expect("site_staging migration");
        sqlx::query(include_str!("migrations/plugin/V001__init.sql"))
            .execute(&self.pool)
            .await
            .expect("plugin migration");
        sqlx::query(include_str!("migrations/plugin_marketplace/V001__init.sql"))
            .execute(&self.pool)
            .await
            .expect("plugin_marketplace migration");
        sqlx::query(include_str!("migrations/collaborators/V001__init.sql"))
            .execute(&self.pool)
            .await
            .expect("collaborators migration");
        sqlx::query(include_str!("migrations/container_registry/V001__init.sql"))
            .execute(&self.pool)
            .await
            .expect("container_registry migration");
        sqlx::query(include_str!("migrations/ai_ops/V001__init.sql"))
            .execute(&self.pool)
            .await
            .expect("ai_ops migration");
        sqlx::query(include_str!("migrations/compliance/V001__init.sql"))
            .execute(&self.pool)
            .await
            .expect("compliance migration");
        sqlx::query(include_str!("migrations/service_manager/V001__init.sql"))
            .execute(&self.pool)
            .await
            .expect("service_manager migration");
        sqlx::query(include_str!("migrations/os_updates/V001__init.sql"))
            .execute(&self.pool)
            .await
            .expect("os_updates migration");
        sqlx::query(include_str!(
            "migrations/synthetic_monitoring/V001__init.sql"
        ))
        .execute(&self.pool)
        .await
        .expect("synthetic_monitoring migration");
        sqlx::query(include_str!("migrations/log_viewer/V001__init.sql"))
            .execute(&self.pool)
            .await
            .expect("log_viewer migration");
        sqlx::query(include_str!("migrations/db_privileges/V001__init.sql"))
            .execute(&self.pool)
            .await
            .expect("db_privileges migration");
        sqlx::query(include_str!("migrations/ip_allocation/V001__init.sql"))
            .execute(&self.pool)
            .await
            .expect("ip_allocation migration");
        sqlx::query(include_str!("migrations/billing/V001__init.sql"))
            .execute(&self.pool)
            .await
            .expect("billing migration");
        sqlx::query(include_str!("migrations/load_balancing/V001__init.sql"))
            .execute(&self.pool)
            .await
            .expect("load_balancing migration");
        sqlx::query(include_str!("migrations/wordpress_toolkit/V001__init.sql"))
            .execute(&self.pool)
            .await
            .expect("wordpress_toolkit migration");
        sqlx::query(include_str!("migrations/wildcard_ssl/V001__init.sql"))
            .execute(&self.pool)
            .await
            .expect("wildcard_ssl migration");
        sqlx::query(include_str!("migrations/app_runtimes/V001__init.sql"))
            .execute(&self.pool)
            .await
            .expect("app_runtimes migration");
        sqlx::query(include_str!("migrations/kernel_isolation/V001__init.sql"))
            .execute(&self.pool)
            .await
            .expect("kernel_isolation migration");
        sqlx::query(include_str!("migrations/dnssec_secondary/V001__init.sql"))
            .execute(&self.pool)
            .await
            .expect("dnssec_secondary migration");
        sqlx::query(include_str!("migrations/mail_filtering/V001__init.sql"))
            .execute(&self.pool)
            .await
            .expect("mail_filtering migration");
        sqlx::query(include_str!("migrations/git_deployment/V001__init.sql"))
            .execute(&self.pool)
            .await
            .expect("git_deployment migration");
        sqlx::query(include_str!(
            "migrations/maintenance_windows/V001__init.sql"
        ))
        .execute(&self.pool)
        .await
        .expect("maintenance_windows migration");
        sqlx::query(include_str!("migrations/container_runtime/V001__init.sql"))
            .execute(&self.pool)
            .await
            .expect("container_runtime migration");
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
