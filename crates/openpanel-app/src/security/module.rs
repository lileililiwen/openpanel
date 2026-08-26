//! Host-security module composition.

use std::{path::PathBuf, sync::Arc};

use openpanel_core::{AppContext, Migration, Module};
use openpanel_domain::security::{FirewallPolicy, LoginThrottlePolicy};

use super::{
    FirewallPort, HostSshKeysService, LoginThrottleService, NftFirewallAdapter, SecurityService,
    SecurityServiceError, SqliteSecurityRepository, SystemClock,
};

/// Stable module name.
pub const MODULE_NAME: &str = "host-security";
/// Security service and durable schema.
pub struct SecurityModule {
    service: Arc<SecurityService>,
    login: Arc<LoginThrottleService>,
    ssh_keys: Arc<HostSshKeysService>,
    migrations: Vec<Migration>,
}
impl SecurityModule {
    /// Production nftables composition.
    pub async fn new(ctx: &AppContext) -> Result<Self, SecurityServiceError> {
        let binary = std::env::var("OPENPANEL__SECURITY__NFT_BINARY")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("/usr/sbin/nft"));
        let state_dir = std::env::var("OPENPANEL__SECURITY__STATE_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from(".openpanel-firewall"));
        Self::with_firewall(ctx, Arc::new(NftFirewallAdapter::new(binary, state_dir))).await
    }

    /// Compose with a fake privileged adapter.
    pub async fn with_firewall(
        ctx: &AppContext,
        firewall: Arc<dyn FirewallPort>,
    ) -> Result<Self, SecurityServiceError> {
        let authorized_keys = std::env::var("OPENPANEL__SECURITY__AUTHORIZED_KEYS")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("/root/.ssh/authorized_keys"));
        Self::with_firewall_and_keys(ctx, firewall, authorized_keys).await
    }

    /// Compose with a fake privileged adapter and an explicit
    /// `authorized_keys` path (tests inject a sandbox path).
    pub async fn with_firewall_and_keys(
        ctx: &AppContext,
        firewall: Arc<dyn FirewallPort>,
        authorized_keys: PathBuf,
    ) -> Result<Self, SecurityServiceError> {
        let repo = Arc::new(SqliteSecurityRepository::new(ctx.db.pool().await));
        let policy = FirewallPolicy::new(vec![22, 8443])?;
        let throttle_policy = LoginThrottlePolicy::new(
            5,
            std::time::Duration::from_secs(300),
            std::time::Duration::from_secs(30),
            std::time::Duration::from_secs(3600),
        )?;
        Ok(Self {
            ssh_keys: Arc::new(HostSshKeysService::new(
                ctx.db.pool().await,
                ctx.audit.clone(),
                authorized_keys,
            )),
            service: Arc::new(SecurityService::with_ports(
                firewall,
                repo.clone(),
                ctx.audit.clone(),
                policy,
            )),
            login: Arc::new(LoginThrottleService::new(
                repo,
                Arc::new(SystemClock),
                throttle_policy,
                Vec::new(),
            )),
            migrations: vec![
                Migration {
                    module: MODULE_NAME,
                    version: "001".into(),
                    description: "firewall rules and login abuse state".into(),
                    sql: crate::migrations::SECURITY_V001.into(),
                },
                Migration {
                    module: MODULE_NAME,
                    version: "002".into(),
                    description: "admin ssh host keys".into(),
                    sql: crate::migrations::SECURITY_V002.into(),
                },
            ],
        })
    }

    /// Shared service.
    pub fn service(&self) -> Arc<SecurityService> {
        self.service.clone()
    }

    /// Shared pre-authentication login-abuse service.
    pub fn login_service(&self) -> Arc<LoginThrottleService> {
        self.login.clone()
    }

    /// Shared admin SSH host-key service.
    pub fn ssh_keys(&self) -> Arc<HostSshKeysService> {
        self.ssh_keys.clone()
    }
}
impl Module for SecurityModule {
    fn name(&self) -> &'static str {
        MODULE_NAME
    }

    fn migrations(&self) -> Vec<Migration> {
        self.migrations.clone()
    }
}
