//! MySQL grant port implementations: the production shell-out
//! adapter and a recording in-memory double for tests.
#![allow(clippy::unwrap_used)]

use std::sync::Mutex;

use async_trait::async_trait;
use openpanel_domain::db_privileges::DbPrivilegeError;

use super::service::MySqlGrantPort;

/// Production adapter shelling out to the `mysql` CLI. Binary
/// discovery mirrors the provisioning conventions; failures map to
/// `DbPrivilegeError`.
pub struct MySqlShellGrantPort {
    /// Path to the mysql client binary.
    pub binary: String,
}

impl MySqlShellGrantPort {
    /// Construct for a binary path (default `/usr/bin/mysql`).
    pub fn new(binary: impl Into<String>) -> Self {
        Self {
            binary: binary.into(),
        }
    }

    async fn exec(&self, sql: &str) -> Result<(), DbPrivilegeError> {
        let output = tokio::process::Command::new(&self.binary)
            .args(["-N", "-B", "-e", sql])
            .output()
            .await
            .map_err(|e| DbPrivilegeError::Persistence(e.to_string()))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(DbPrivilegeError::Persistence(format!(
                "mysql: {}",
                stderr.chars().take(256).collect::<String>()
            )));
        }
        Ok(())
    }
}

#[async_trait]
impl MySqlGrantPort for MySqlShellGrantPort {
    async fn current_hosts(
        &self,
        user: &str,
        _database: &str,
    ) -> Result<Vec<String>, DbPrivilegeError> {
        // `mysql.user` Host column for the account's user part.
        let output = tokio::process::Command::new(&self.binary)
            .args([
                "-N",
                "-B",
                "-e",
                &format!("SELECT Host FROM mysql.user WHERE User = '{user}'"),
            ])
            .output()
            .await
            .map_err(|e| DbPrivilegeError::Persistence(e.to_string()))?;
        if !output.status.success() {
            return Err(DbPrivilegeError::Persistence("mysql query failed".into()));
        }
        Ok(String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(str::to_string)
            .collect())
    }

    async fn create_user_host(
        &self,
        user: &str,
        host: &str,
        database: &str,
    ) -> Result<(), DbPrivilegeError> {
        self.exec(&format!(
            "CREATE USER IF NOT EXISTS '{user}'@'{host}'; GRANT ALL PRIVILEGES ON `{database}`.* TO '{user}'@'{host}';"
        ))
        .await
    }

    async fn drop_user_host(&self, user: &str, host: &str) -> Result<(), DbPrivilegeError> {
        self.exec(&format!("DROP USER IF EXISTS '{user}'@'{host}';"))
            .await
    }
}

/// Recording in-memory port for tests: current hosts are
/// configurable, every call is recorded.
#[derive(Default)]
pub struct MemoryGrantPort {
    /// Hosts reported by `current_hosts`.
    pub current: Mutex<Vec<String>>,
    /// Executed calls in order ("create:host" / "drop:host").
    pub calls: Mutex<Vec<String>>,
}

impl MemoryGrantPort {
    /// Build with an initial current-host set.
    pub fn new(current: Vec<String>) -> Self {
        Self {
            current: Mutex::new(current),
            calls: Mutex::new(Vec::new()),
        }
    }

    /// Recorded calls.
    pub fn recorded(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }
}

#[async_trait]
impl MySqlGrantPort for MemoryGrantPort {
    async fn current_hosts(
        &self,
        _user: &str,
        _database: &str,
    ) -> Result<Vec<String>, DbPrivilegeError> {
        Ok(self.current.lock().unwrap().clone())
    }

    async fn create_user_host(
        &self,
        _user: &str,
        host: &str,
        _database: &str,
    ) -> Result<(), DbPrivilegeError> {
        self.calls.lock().unwrap().push(format!("create:{host}"));
        self.current.lock().unwrap().push(host.to_string());
        Ok(())
    }

    async fn drop_user_host(&self, _user: &str, host: &str) -> Result<(), DbPrivilegeError> {
        self.calls.lock().unwrap().push(format!("drop:{host}"));
        self.current.lock().unwrap().retain(|h| h != host);
        Ok(())
    }
}
