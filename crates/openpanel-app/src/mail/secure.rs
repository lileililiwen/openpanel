//! DKIM key custody and isolated Postfix/Dovecot configuration transactions.

use std::{
    fs::{self, OpenOptions},
    io::Write,
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    sync::Arc,
};

use async_trait::async_trait;
use tokio::process::Command;
use uuid::Uuid;

use super::{MailConfigurator, MailDomain, MailServiceError};

/// AES-256-GCM custody for DKIM private keys.
pub struct DkimKeyCustody([u8; 32]);

impl DkimKeyCustody {
    /// Construct from the installation's 256-bit master key.
    pub fn new(key: &[u8]) -> Result<Self, MailServiceError> {
        let key = key
            .try_into()
            .map_err(|_| MailServiceError::Configuration)?;
        Ok(Self(key))
    }

    /// Encrypt private key material for database storage.
    pub fn encrypt(&self, private_key: &str) -> Result<String, MailServiceError> {
        crate::databases::crypto::encrypt_to_storage(&self.0, private_key)
            .map_err(|_| MailServiceError::Configuration)
    }

    /// Decrypt directly into an atomically replaced `0600` service key file.
    pub fn materialize(&self, encrypted: &str, path: &Path) -> Result<(), MailServiceError> {
        let private_key = crate::databases::crypto::decrypt_from_storage(&self.0, encrypted)
            .map_err(|_| MailServiceError::Configuration)?;
        let parent = path.parent().ok_or(MailServiceError::Configuration)?;
        fs::create_dir_all(parent).map_err(|_| MailServiceError::Configuration)?;
        let temporary = parent.join(format!(".dkim-{}.tmp", Uuid::new_v4()));
        let result = (|| {
            let mut file = OpenOptions::new()
                .create_new(true)
                .write(true)
                .mode(0o600)
                .open(&temporary)
                .map_err(|_| MailServiceError::Configuration)?;
            file.write_all(private_key.as_bytes())
                .and_then(|()| file.sync_all())
                .map_err(|_| MailServiceError::Configuration)?;
            fs::rename(&temporary, path).map_err(|_| MailServiceError::Configuration)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result
    }
}

/// Fixed-command boundary used to validate candidates and reload mail services.
#[async_trait]
pub trait MailConfigControl: Send + Sync {
    /// Validate both candidate include files without mutating active state.
    async fn validate(&self, postfix: &Path, dovecot: &Path) -> Result<(), MailServiceError>;
    /// Reload Postfix and Dovecot after an atomic swap.
    async fn reload(&self) -> Result<(), MailServiceError>;
}

/// Host command adapter. Command names and arguments are not caller-controlled.
pub struct SystemMailConfigControl;

#[async_trait]
impl MailConfigControl for SystemMailConfigControl {
    async fn validate(&self, _postfix: &Path, dovecot: &Path) -> Result<(), MailServiceError> {
        let postfix_ok = Command::new("postfix")
            .arg("check")
            .status()
            .await
            .map(|status| status.success())
            .unwrap_or(false);
        let dovecot_ok = Command::new("doveconf")
            .arg("-c")
            .arg(dovecot)
            .arg("-n")
            .status()
            .await
            .map(|status| status.success())
            .unwrap_or(false);
        if postfix_ok && dovecot_ok {
            Ok(())
        } else {
            Err(MailServiceError::Configuration)
        }
    }

    async fn reload(&self) -> Result<(), MailServiceError> {
        for service in ["postfix", "dovecot"] {
            let success = Command::new("systemctl")
                .args(["reload", service])
                .status()
                .await
                .map(|status| status.success())
                .unwrap_or(false);
            if !success {
                return Err(MailServiceError::Configuration);
            }
        }
        Ok(())
    }
}

/// Isolated include writer with validate, atomic apply, reload, and rollback.
pub struct FilesystemMailConfigurator {
    root: PathBuf,
    control: Arc<dyn MailConfigControl>,
}

impl FilesystemMailConfigurator {
    /// Construct for a dedicated OpenPanel-owned include directory.
    pub fn new(root: impl Into<PathBuf>, control: Arc<dyn MailConfigControl>) -> Self {
        Self {
            root: root.into(),
            control,
        }
    }
}

#[async_trait]
impl MailConfigurator for FilesystemMailConfigurator {
    async fn validate_apply_reload(&self, domain: &MailDomain) -> Result<(), MailServiceError> {
        fs::create_dir_all(&self.root).map_err(|_| MailServiceError::Configuration)?;
        let postfix = self.root.join("postfix-openpanel.cf");
        let dovecot = self.root.join("dovecot-openpanel.conf");
        let postfix_candidate = self.root.join(".postfix-openpanel.candidate");
        let dovecot_candidate = self.root.join(".dovecot-openpanel.candidate");
        write_candidate(&postfix_candidate, &postfix_config(domain))?;
        write_candidate(&dovecot_candidate, &dovecot_config(domain))?;
        if let Err(error) = self
            .control
            .validate(&postfix_candidate, &dovecot_candidate)
            .await
        {
            cleanup(&[&postfix_candidate, &dovecot_candidate]);
            return Err(error);
        }
        let old_postfix = fs::read(&postfix).ok();
        let old_dovecot = fs::read(&dovecot).ok();
        fs::rename(&postfix_candidate, &postfix).map_err(|_| MailServiceError::Configuration)?;
        if fs::rename(&dovecot_candidate, &dovecot).is_err() {
            restore(&postfix, old_postfix.as_deref())?;
            cleanup(&[&dovecot_candidate]);
            return Err(MailServiceError::Configuration);
        }
        if let Err(error) = self.control.reload().await {
            restore(&postfix, old_postfix.as_deref())?;
            restore(&dovecot, old_dovecot.as_deref())?;
            let _ = self.control.reload().await;
            return Err(error);
        }
        Ok(())
    }
}

fn write_candidate(path: &Path, value: &str) -> Result<(), MailServiceError> {
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .mode(0o640)
        .open(path)
        .map_err(|_| MailServiceError::Configuration)?;
    file.write_all(value.as_bytes())
        .and_then(|()| file.sync_all())
        .map_err(|_| MailServiceError::Configuration)
}

fn restore(path: &Path, previous: Option<&[u8]>) -> Result<(), MailServiceError> {
    match previous {
        Some(bytes) => fs::write(path, bytes).map_err(|_| MailServiceError::Configuration),
        None => match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err(MailServiceError::Configuration),
        },
    }
}

fn cleanup(paths: &[&Path]) {
    for path in paths {
        let _ = fs::remove_file(path);
    }
}

fn postfix_config(domain: &MailDomain) -> String {
    format!(
        "# OpenPanel isolated include\nvirtual_mailbox_domains = {}\nsmtpd_tls_security_level = may\nsmtpd_tls_auth_only = yes\nsmtpd_relay_restrictions = permit_mynetworks, permit_sasl_authenticated, reject_unauth_destination\n",
        domain.name
    )
}

fn dovecot_config(domain: &MailDomain) -> String {
    format!(
        "# OpenPanel isolated include for {}\nssl = required\ndisable_plaintext_auth = yes\nprotocols = imap lmtp\n",
        domain.name
    )
}
