//! Fixed-argv systemd, journal, and deterministic memory adapters.

use std::{path::PathBuf, sync::Mutex, time::Duration};

use async_trait::async_trait;
use openpanel_domain::system_services::ServiceAction;

use super::{ControllerStatus, JournalPort, ServiceController, ServiceManagerError};

/// `systemctl` adapter with no shell and a bounded deadline.
pub struct SystemdController {
    binary: PathBuf,
    timeout: Duration,
}
impl SystemdController {
    /// Construct with an explicit executable.
    pub fn new(binary: PathBuf, timeout: Duration) -> Self {
        Self { binary, timeout }
    }

    async fn run(&self, args: &[&str]) -> Result<std::process::Output, ServiceManagerError> {
        tokio::time::timeout(
            self.timeout,
            tokio::process::Command::new(&self.binary)
                .args(args)
                .output(),
        )
        .await
        .map_err(|_| ServiceManagerError::Controller)?
        .map_err(|_| ServiceManagerError::Controller)
    }
}
#[async_trait]
impl ServiceController for SystemdController {
    async fn status(&self, unit: &str) -> Result<ControllerStatus, ServiceManagerError> {
        let output = self
            .run(&[
                "show",
                unit,
                "--property=LoadState,ActiveState,SubState,UnitFileState",
                "--no-pager",
            ])
            .await?;
        if !output.status.success() {
            return Err(ServiceManagerError::Controller);
        }
        let text = String::from_utf8_lossy(&output.stdout);
        let value = |key: &str, fallback: &str| {
            text.lines()
                .find_map(|line| line.strip_prefix(&format!("{key}=")))
                .unwrap_or(fallback)
                .to_string()
        };
        Ok(ControllerStatus {
            load_state: value("LoadState", "loaded"),
            active_state: value("ActiveState", "active"),
            sub_state: value("SubState", "running"),
            enabled: value("UnitFileState", "enabled") == "enabled",
        })
    }

    async fn action(&self, unit: &str, action: ServiceAction) -> Result<(), ServiceManagerError> {
        if self.run(&[action.verb(), unit]).await?.status.success() {
            Ok(())
        } else {
            Err(ServiceManagerError::Controller)
        }
    }

    async fn probe(&self, unit: &str) -> Result<bool, ServiceManagerError> {
        Ok(self.status(unit).await?.active_state == "active")
    }
}

/// Fixed-unit `journalctl` adapter.
pub struct JournalctlAdapter {
    binary: PathBuf,
    timeout: Duration,
}
impl JournalctlAdapter {
    /// Construct with an explicit executable and deadline.
    pub fn new(binary: PathBuf, timeout: Duration) -> Self {
        Self { binary, timeout }
    }
}
#[async_trait]
impl JournalPort for JournalctlAdapter {
    async fn entries(&self, unit: &str, limit: usize) -> Result<Vec<String>, ServiceManagerError> {
        let count = limit.clamp(1, 500).to_string();
        let output = tokio::time::timeout(
            self.timeout,
            tokio::process::Command::new(&self.binary)
                .args(["-u", unit, "-n", &count, "--no-pager", "--output=cat"])
                .output(),
        )
        .await
        .map_err(|_| ServiceManagerError::Controller)?
        .map_err(|_| ServiceManagerError::Controller)?;
        if !output.status.success() {
            return Err(ServiceManagerError::Controller);
        }
        Ok(String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(str::to_string)
            .collect())
    }
}

/// In-memory controller for HTTP tests.
pub struct MemoryServiceController {
    state: Mutex<ControllerStatus>,
}
impl Default for MemoryServiceController {
    fn default() -> Self {
        Self {
            state: Mutex::new(ControllerStatus::active()),
        }
    }
}
#[async_trait]
impl ServiceController for MemoryServiceController {
    async fn status(&self, _: &str) -> Result<ControllerStatus, ServiceManagerError> {
        self.state
            .lock()
            .map(|value| value.clone())
            .map_err(|_| ServiceManagerError::Controller)
    }

    async fn action(&self, _: &str, action: ServiceAction) -> Result<(), ServiceManagerError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| ServiceManagerError::Controller)?;
        match action {
            ServiceAction::Stop => {
                state.active_state = "inactive".into();
                state.sub_state = "dead".into();
            }
            ServiceAction::Disable => state.enabled = false,
            ServiceAction::Enable => state.enabled = true,
            _ => {
                state.active_state = "active".into();
                state.sub_state = "running".into();
            }
        }
        Ok(())
    }

    async fn probe(&self, _: &str) -> Result<bool, ServiceManagerError> {
        Ok(true)
    }
}
/// Empty deterministic journal.
pub struct EmptyJournal;
#[async_trait]
impl JournalPort for EmptyJournal {
    async fn entries(&self, _: &str, _: usize) -> Result<Vec<String>, ServiceManagerError> {
        Ok(vec![])
    }
}
