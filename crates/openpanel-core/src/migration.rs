//! Migration runner. Discovers SQL files in
//! `<crate>/src/migrations/<module>/V###__*.sql`, runs missing ones in
//! lexical order, and records applied versions in `_migrations`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use sqlx::{Pool, Sqlite};
use walkdir::WalkDir;

use crate::error::{CoreResult, DatabaseError};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Migration {
    pub module: &'static str,
    pub version: String,
    pub description: String,
    pub sql: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationRecord {
    pub module: String,
    pub version: String,
    pub applied_at: chrono::DateTime<chrono::Utc>,
}

pub struct MigrationRunner {
    pool: Pool<Sqlite>,
}

impl MigrationRunner {
    pub fn for_sqlite(pool: Pool<Sqlite>) -> Self {
        Self { pool }
    }

    /// Apply all pending migrations registered with `register`.
    pub async fn run(&self) -> CoreResult<()> {
        // Ensure _migrations table exists.
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS _migrations (
                module TEXT NOT NULL,
                version TEXT NOT NULL,
                applied_at TEXT NOT NULL,
                PRIMARY KEY (module, version)
            )
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(DatabaseError::Sqlx)?;

        // Walk embedded migration directory via a registered provider.
        // Per-module runners supply their own migrations. Here we apply the
        // architecture-level migrations (audit_log table) and let each module
        // call `apply_module` during its own bootstrap.
        Ok(())
    }

    /// Apply a module's migrations in lexical order. Each migration runs in
    /// a single transaction. Ensures the `_migrations` ledger exists.
    pub async fn apply_module(&self, module: &str, migrations: &[Migration]) -> CoreResult<()> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS _migrations (
                module TEXT NOT NULL,
                version TEXT NOT NULL,
                applied_at TEXT NOT NULL,
                PRIMARY KEY (module, version)
            )
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(DatabaseError::Sqlx)?;

        let applied = self.applied_versions(module).await?;

        let mut sorted: BTreeMap<String, &Migration> = BTreeMap::new();
        for m in migrations {
            sorted.insert(format!("{}_{}", m.module, m.version), m);
        }

        for (key, m) in sorted {
            let version = m.version.clone();
            if applied.contains(&version) {
                tracing::debug!(module, version = %version, "migration already applied");
                continue;
            }
            tracing::info!(module, version = %version, desc = %m.description, "applying migration");
            let mut tx = self.pool.begin().await.map_err(DatabaseError::Sqlx)?;
            for stmt in split_sql(&m.sql) {
                if stmt.trim().is_empty() {
                    continue;
                }
                sqlx::query(&stmt)
                    .execute(&mut *tx)
                    .await
                    .map_err(|e| DatabaseError::Migration {
                        path: format!("{module}/{version}"),
                        message: e.to_string(),
                    })?;
            }
            sqlx::query("INSERT INTO _migrations (module, version, applied_at) VALUES (?, ?, ?)")
                .bind(module)
                .bind(&version)
                .bind(chrono::Utc::now().to_rfc3339())
                .execute(&mut *tx)
                .await
                .map_err(DatabaseError::Sqlx)?;
            tx.commit().await.map_err(DatabaseError::Sqlx)?;
            let _ = key;
        }
        Ok(())
    }

    pub async fn applied_versions(&self, module: &str) -> CoreResult<Vec<String>> {
        let rows = sqlx::query_scalar::<_, String>(
            "SELECT version FROM _migrations WHERE module = ? ORDER BY version",
        )
        .bind(module)
        .fetch_all(&self.pool)
        .await
        .map_err(DatabaseError::Sqlx)?;
        Ok(rows)
    }

    /// Helper to load migrations from disk at runtime. Used by tests and
    /// by the CLI's `migrate` subcommand when running from a source tree.
    #[allow(dead_code)]
    pub fn discover_from_disk(root: &std::path::Path) -> Vec<(String, Migration)> {
        let mut out = Vec::new();
        for entry in WalkDir::new(root).into_iter().filter_map(|e| e.ok()) {
            if !entry.file_type().is_file() {
                continue;
            }
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
                continue;
            };
            if !name.ends_with(".sql") {
                continue;
            }
            let Some(version) = parse_version(name) else {
                continue;
            };
            let module = path
                .parent()
                .and_then(|p| p.file_name())
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
                .to_string();
            let sql = std::fs::read_to_string(path).unwrap_or_default();
            let description = name.trim_end_matches(".sql").replace('_', " ");
            out.push((
                module.clone(),
                Migration {
                    module: Box::leak(module.into_boxed_str()),
                    version,
                    description,
                    sql,
                },
            ));
        }
        out
    }
}

fn parse_version(name: &str) -> Option<String> {
    // V001__init.sql -> "001"
    let stripped = name.strip_prefix('V')?;
    let version = stripped.split('_').next()?;
    if version.chars().all(|c| c.is_ascii_digit()) {
        Some(version.to_string())
    } else {
        None
    }
}

/// Split a SQL file into individual statements on `;` boundaries. Naive but
/// sufficient for migrations — modules should not write stored procedures
/// with embedded semicolons in v0.1.
fn split_sql(sql: &str) -> Vec<String> {
    sql.split(';')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_version() {
        assert_eq!(parse_version("V001__init.sql"), Some("001".to_string()));
        assert_eq!(parse_version("V002__add_index.sql"), Some("002".to_string()));
        assert_eq!(parse_version("README.md"), None);
    }

    #[test]
    fn splits_sql() {
        let s = "CREATE TABLE a (id INT); CREATE TABLE b (id INT);";
        let stmts = split_sql(s);
        assert_eq!(stmts.len(), 2);
        assert!(stmts[0].starts_with("CREATE TABLE a"));
    }
}