//! Mail module composition and safe deterministic adapters.
use std::sync::Arc;

use async_trait::async_trait;
use openpanel_core::{AppContext, Migration, Module};
use openpanel_domain::mail::MailDomainName;
use tokio::process::Command;

use super::{
    BackupHook, FilesystemMailConfigurator, MailConfigurator, MailDomain, MailRepository,
    MailService, MailServiceError, MemoryMailRepository, Readiness, ReadinessPort,
    SqliteMailRepository, SystemMailConfigControl,
};

/// Deterministic isolated-config adapter for tests.
pub struct FakeMailConfigurator;
#[async_trait]
impl MailConfigurator for FakeMailConfigurator {
    async fn validate_apply_reload(&self, _domain: &MailDomain) -> Result<(), MailServiceError> {
        Ok(())
    }
}
struct Ready;
#[async_trait]
impl ReadinessPort for Ready {
    async fn check(&self, _domain: &MailDomainName) -> Result<Readiness, MailServiceError> {
        Ok(Readiness::ready())
    }
}
struct SystemReadiness;
#[async_trait]
impl ReadinessPort for SystemReadiness {
    async fn check(&self, _domain: &MailDomainName) -> Result<Readiness, MailServiceError> {
        let hostname = std::env::var("OPENPANEL__MAIL__HOSTNAME").unwrap_or_default();
        let storage =
            std::env::var("OPENPANEL__MAIL__STORAGE_ROOT").unwrap_or_else(|_| "/var/vmail".into());
        let tls = std::env::var("OPENPANEL__MAIL__TLS_CERT").unwrap_or_default();
        let postfix = command_ok("postfix", &["check"]).await;
        let dovecot = command_ok("doveconf", &["-n"]).await;
        let storage_ready = std::path::Path::new(&storage).is_dir();
        let tls_ready = !tls.is_empty() && std::path::Path::new(&tls).is_file();
        let hostname_ready = MailDomainName::new(&hostname).is_ok();
        let ready = postfix && dovecot && storage_ready && tls_ready && hostname_ready;
        let mut checks = Vec::new();
        for (label, passed) in [
            ("postfix", postfix),
            ("dovecot", dovecot),
            ("storage", storage_ready),
            ("tls", tls_ready),
            ("hostname", hostname_ready),
        ] {
            checks.push(format!("{label}:{}", if passed { "ok" } else { "missing" }));
        }
        Ok(Readiness {
            ready,
            expected_mx: hostname_ready.then(|| format!("MX 10 {hostname}")),
            checks,
            can_acknowledge: false,
        })
    }
}

async fn command_ok(program: &str, arguments: &[&str]) -> bool {
    Command::new(program)
        .args(arguments)
        .status()
        .await
        .map(|status| status.success())
        .unwrap_or(false)
}
struct Backup;
#[async_trait]
impl BackupHook for Backup {
    async fn register_domain(&self, _domain_id: uuid::Uuid) -> Result<(), MailServiceError> {
        Ok(())
    }
}
/// Hosted mail module.
pub struct MailModule {
    service: Arc<MailService>,
    migrations: Vec<Migration>,
}
impl MailModule {
    /// Compose SQLite-backed mail metadata.
    pub async fn new(ctx: &AppContext) -> Result<Self, MailServiceError> {
        if std::env::var("OPENPANEL__MAIL__ADAPTER").as_deref() == Ok("fake") {
            return Self::compose_with_config_and_readiness(
                ctx,
                Arc::new(SqliteMailRepository::new(ctx.db.pool().await)),
                Arc::new(FakeMailConfigurator),
                Arc::new(Ready),
            )
            .await;
        }
        let root = std::env::var("OPENPANEL__MAIL__CONFIG_ROOT")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| std::path::PathBuf::from("/etc/openpanel/mail"));
        Self::compose_with_config_and_readiness(
            ctx,
            Arc::new(SqliteMailRepository::new(ctx.db.pool().await)),
            Arc::new(FilesystemMailConfigurator::new(
                root,
                Arc::new(SystemMailConfigControl),
            )),
            Arc::new(SystemReadiness),
        )
        .await
    }

    /// Compose deterministic in-memory mail.
    pub async fn memory(ctx: &AppContext) -> Result<Self, MailServiceError> {
        Self::compose_with_config_and_readiness(
            ctx,
            Arc::new(MemoryMailRepository::default()),
            Arc::new(FakeMailConfigurator),
            Arc::new(Ready),
        )
        .await
    }

    async fn compose_with_config_and_readiness(
        ctx: &AppContext,
        repo: Arc<dyn MailRepository>,
        config: Arc<dyn MailConfigurator>,
        readiness: Arc<dyn ReadinessPort>,
    ) -> Result<Self, MailServiceError> {
        Ok(Self {
            service: Arc::new(MailService::new(
                repo,
                config,
                readiness,
                Arc::new(Backup),
                ctx.audit.clone(),
            )),
            migrations: vec![Migration {
                module: "mail",
                version: "001".into(),
                description: "mail domains, mailboxes, and aliases".into(),
                sql: crate::migrations::MAIL_V001.into(),
            }],
        })
    }

    /// Shared service.
    pub fn service(&self) -> Arc<MailService> {
        self.service.clone()
    }
}
impl Module for MailModule {
    fn name(&self) -> &'static str {
        "mail"
    }

    fn migrations(&self) -> Vec<Migration> {
        self.migrations.clone()
    }
}
