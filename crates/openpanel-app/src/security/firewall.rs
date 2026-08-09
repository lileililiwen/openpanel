//! nftables and in-memory firewall adapters.

use std::{path::PathBuf, sync::Mutex};

use async_trait::async_trait;

use super::{FirewallPort, SecurityServiceError};

/// Privileged nft command adapter using argument arrays rather than a shell.
pub struct NftFirewallAdapter {
    binary: PathBuf,
    state_dir: PathBuf,
}
impl NftFirewallAdapter {
    /// Construct with explicit binary and OpenPanel-owned state directory.
    pub fn new(binary: PathBuf, state_dir: PathBuf) -> Self {
        Self { binary, state_dir }
    }

    async fn run_file(
        &self,
        check: bool,
        path: &std::path::Path,
    ) -> Result<bool, SecurityServiceError> {
        let mut command = tokio::process::Command::new(&self.binary);
        if check {
            command.arg("--check");
        }
        let status = command
            .arg("-f")
            .arg(path)
            .status()
            .await
            .map_err(|_| SecurityServiceError::Firewall)?;
        Ok(status.success())
    }
}
#[async_trait]
impl FirewallPort for NftFirewallAdapter {
    async fn supported(&self) -> Result<bool, SecurityServiceError> {
        Ok(self.binary.is_file())
    }

    async fn check(&self, candidate: &str) -> Result<bool, SecurityServiceError> {
        tokio::fs::create_dir_all(&self.state_dir)
            .await
            .map_err(|_| SecurityServiceError::Firewall)?;
        let path = self.state_dir.join("candidate.nft");
        tokio::fs::write(&path, candidate)
            .await
            .map_err(|_| SecurityServiceError::Firewall)?;
        self.run_file(true, &path).await
    }

    async fn apply(&self, candidate: &str) -> Result<(), SecurityServiceError> {
        tokio::fs::create_dir_all(&self.state_dir)
            .await
            .map_err(|_| SecurityServiceError::Firewall)?;
        let active = self.state_dir.join("active.nft");
        let last = self.state_dir.join("last-good.nft");
        if active.exists() {
            tokio::fs::copy(&active, &last)
                .await
                .map_err(|_| SecurityServiceError::Firewall)?;
        }
        let candidate_path = self.state_dir.join("candidate.nft");
        tokio::fs::write(&candidate_path, candidate)
            .await
            .map_err(|_| SecurityServiceError::Firewall)?;
        if !self.run_file(false, &candidate_path).await? {
            return Err(SecurityServiceError::Firewall);
        }
        tokio::fs::rename(candidate_path, active)
            .await
            .map_err(|_| SecurityServiceError::Firewall)?;
        Ok(())
    }

    async fn verify(&self) -> Result<bool, SecurityServiceError> {
        Ok(true)
    }

    async fn rollback(&self) -> Result<(), SecurityServiceError> {
        let last = self.state_dir.join("last-good.nft");
        if last.exists() && !self.run_file(false, &last).await? {
            return Err(SecurityServiceError::Firewall);
        }
        Ok(())
    }
}

/// Deterministic non-privileged adapter for HTTP integration tests.
#[derive(Default)]
pub struct MemoryFirewall {
    active: Mutex<String>,
}
#[async_trait]
impl FirewallPort for MemoryFirewall {
    async fn supported(&self) -> Result<bool, SecurityServiceError> {
        Ok(true)
    }

    async fn check(&self, candidate: &str) -> Result<bool, SecurityServiceError> {
        Ok(candidate.starts_with("table inet openpanel"))
    }

    async fn apply(&self, candidate: &str) -> Result<(), SecurityServiceError> {
        *self
            .active
            .lock()
            .map_err(|_| SecurityServiceError::Firewall)? = candidate.to_string();
        Ok(())
    }

    async fn verify(&self) -> Result<bool, SecurityServiceError> {
        Ok(true)
    }

    async fn rollback(&self) -> Result<(), SecurityServiceError> {
        Ok(())
    }
}
