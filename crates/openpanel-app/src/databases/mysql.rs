//! MySQL shell-out wrapper. All MySQL mutations are issued via the
//! `mysql` CLI so the panel does not need a native MySQL client library.
//!
//! All methods are async and use `tokio::process::Command` so they
//! integrate with the tokio runtime without blocking.

use std::path::PathBuf;
use tokio::process::Command;

use openpanel_domain::databases::error::DatabaseError;

pub struct MySqlClient {
    binary: PathBuf,
    admin_user: String,
}

impl MySqlClient {
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

    pub fn available(&self) -> bool {
        self.binary.exists()
    }

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

    pub async fn grant_all(
        &self,
        db: &str,
        user: &str,
        host: &str,
    ) -> Result<(), DatabaseError> {
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

    pub async fn drop_database(&self, name: &str) -> Result<(), DatabaseError> {
        if !self.available() {
            return Err(DatabaseError::MysqlMissing);
        }
        let sql = format!("DROP DATABASE IF EXISTS `{}`;", name.replace('`', "``"));
        self.run(&sql).await
    }

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