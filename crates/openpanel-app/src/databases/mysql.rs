//! MySQL shell-out wrapper. All MySQL mutations are issued via the
//! `mysql` CLI so the panel does not need a native MySQL client library.
//!
//! All methods are async and use `tokio::process::Command` so they
//! integrate with the tokio runtime without blocking.

use std::path::PathBuf;

use openpanel_domain::databases::error::DatabaseError;
use tokio::process::Command;

/// Thin wrapper around the `mysql` CLI used to perform MySQL mutations
/// without a native client library.
pub struct MySqlClient {
    binary: PathBuf,
    admin_user: String,
}

impl MySqlClient {
    /// Construct a client bound to a specific `mysql` binary path and admin user.
    pub fn new(binary: impl Into<PathBuf>, admin_user: impl Into<String>) -> Self {
        Self {
            binary: binary.into(),
            admin_user: admin_user.into(),
        }
    }

    /// Detect the mysql CLI in PATH; returns None if missing.
    pub fn detect() -> Option<PathBuf> {
        let out = std::process::Command::new("which")
            .arg("mysql")
            .output()
            .ok()?;
        if out.status.success() {
            let s = String::from_utf8(out.stdout).ok()?;
            let p = s.trim();
            if !p.is_empty() {
                return Some(PathBuf::from(p));
            }
        }
        None
    }

    /// Returns true when the configured `mysql` binary exists on disk.
    pub fn available(&self) -> bool {
        self.binary.exists()
    }

    /// Create a new MySQL database with the given character set.
    pub async fn create_database(&self, name: &str, charset: &str) -> Result<(), DatabaseError> {
        if !self.available() {
            return Err(DatabaseError::MysqlMissing);
        }
        let sql = format!(
            "CREATE DATABASE `{name}` CHARACTER SET {charset};",
            name = name.replace('`', "``"),
            charset = charset
        );
        self.run(&sql).await
    }

    /// Create a MySQL user account identified by the given password.
    pub async fn create_user(
        &self,
        user: &str,
        host: &str,
        password: &str,
    ) -> Result<(), DatabaseError> {
        if !self.available() {
            return Err(DatabaseError::MysqlMissing);
        }
        let safe_pw = password.replace('\'', "''");
        let sql = format!(
            "CREATE USER `{}`@`{}` IDENTIFIED BY '{}';",
            user.replace('`', "``"),
            host.replace('`', "``"),
            safe_pw
        );
        self.run(&sql).await
    }

    /// Grant full privileges on the given database to a user/host pair.
    pub async fn grant_all(&self, db: &str, user: &str, host: &str) -> Result<(), DatabaseError> {
        if !self.available() {
            return Err(DatabaseError::MysqlMissing);
        }
        let sql = format!(
            "GRANT ALL PRIVILEGES ON `{}`.* TO `{}`@`{}`;",
            db.replace('`', "``"),
            user.replace('`', "``"),
            host.replace('`', "``")
        );
        self.run(&sql).await
    }

    /// Rotate the password for an existing MySQL user.
    pub async fn change_password(
        &self,
        user: &str,
        host: &str,
        new_password: &str,
    ) -> Result<(), DatabaseError> {
        if !self.available() {
            return Err(DatabaseError::MysqlMissing);
        }
        let safe_pw = new_password.replace('\'', "''");
        let sql = format!(
            "ALTER USER `{}`@`{}` IDENTIFIED BY '{}';",
            user.replace('`', "``"),
            host.replace('`', "``"),
            safe_pw
        );
        self.run(&sql).await
    }

    /// Drop a MySQL database if it exists.
    pub async fn drop_database(&self, name: &str) -> Result<(), DatabaseError> {
        if !self.available() {
            return Err(DatabaseError::MysqlMissing);
        }
        let sql = format!("DROP DATABASE IF EXISTS `{}`;", name.replace('`', "``"));
        self.run(&sql).await
    }

    /// Drop a MySQL user account if it exists.
    pub async fn drop_user(&self, user: &str, host: &str) -> Result<(), DatabaseError> {
        if !self.available() {
            return Err(DatabaseError::MysqlMissing);
        }
        let sql = format!(
            "DROP USER IF EXISTS `{}`@`{}`;",
            user.replace('`', "``"),
            host.replace('`', "``")
        );
        self.run(&sql).await
    }

    async fn run(&self, sql: &str) -> Result<(), DatabaseError> {
        let out = Command::new(&self.binary)
            .arg("-u")
            .arg(&self.admin_user)
            .arg("-e")
            .arg(sql)
            .output()
            .await
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    DatabaseError::MysqlMissing
                } else {
                    DatabaseError::Io(e.to_string())
                }
            })?;
        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
            return Err(DatabaseError::MysqlError(stderr));
        }
        Ok(())
    }
}
