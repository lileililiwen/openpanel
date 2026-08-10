//! Fixed-argument APT adapter for explicitly supported Debian-family hosts.

use std::{collections::BTreeSet, sync::Arc};

use async_trait::async_trait;
use openpanel_domain::software_center::{PackageId, PlanAction, SupportedPlatform};
use sha2::{Digest, Sha256};
use tokio::process::Command;

use super::{HostSnapshot, PackageManager, SoftwareCenterError};

const OUTPUT_LIMIT: usize = 8_192;

/// Bounded command result with no command line or environment attached.
#[derive(Debug, Clone)]
pub struct CommandResult {
    /// Whether the fixed command completed successfully.
    pub success: bool,
    /// Bounded, control-character-sanitized diagnostic output.
    pub output: String,
    raw_output: String,
}
impl CommandResult {
    /// Construct a successful deterministic result.
    pub fn success(output: &str) -> Self {
        Self {
            success: true,
            output: bounded(output),
            raw_output: raw_bounded(output),
        }
    }

    fn failure(output: &str) -> Self {
        Self {
            success: false,
            output: bounded(output),
            raw_output: raw_bounded(output),
        }
    }
}

fn raw_bounded(output: &str) -> String {
    output
        .chars()
        .filter(|character| !character.is_control() || matches!(character, '\n' | '\t'))
        .take(4 * 1024 * 1024)
        .collect()
}

fn bounded(output: &str) -> String {
    output
        .lines()
        .map(|line| {
            let lowercase = line.to_ascii_lowercase();
            if ["password", "passwd", "secret", "token", "authorization"]
                .iter()
                .any(|marker| lowercase.contains(marker))
            {
                "[redacted]"
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
        .chars()
        .filter(|character| !character.is_control() || matches!(character, '\n' | '\t'))
        .take(OUTPUT_LIMIT)
        .collect()
}

/// Command execution boundary used only with adapter-owned programs and arguments.
#[async_trait]
pub trait PackageCommand: Send + Sync {
    /// Execute one fixed command and return bounded output.
    async fn run(
        &self,
        program: &'static str,
        arguments: &[String],
    ) -> Result<CommandResult, SoftwareCenterError>;
}

/// Tokio process executor with an empty environment and piped output.
pub struct TokioPackageCommand;
#[async_trait]
impl PackageCommand for TokioPackageCommand {
    async fn run(
        &self,
        program: &'static str,
        arguments: &[String],
    ) -> Result<CommandResult, SoftwareCenterError> {
        let output = Command::new(program)
            .args(arguments)
            .env_clear()
            .env("DEBIAN_FRONTEND", "noninteractive")
            .output()
            .await
            .map_err(|_| SoftwareCenterError::Package)?;
        let diagnostic = if output.status.success() {
            String::from_utf8_lossy(&output.stdout)
        } else {
            String::from_utf8_lossy(&output.stderr)
        };
        Ok(if output.status.success() {
            CommandResult::success(&diagnostic)
        } else {
            CommandResult::failure(&diagnostic)
        })
    }
}

/// APT package adapter whose request data can only contain validated package IDs.
pub struct AptPackageManager {
    command: Arc<dyn PackageCommand>,
}
impl AptPackageManager {
    /// Construct from the fixed-command runner.
    pub fn new(command: Arc<dyn PackageCommand>) -> Self {
        Self { command }
    }

    async fn package_action(
        &self,
        operation: &str,
        package: &PackageId,
        extra: &[&str],
    ) -> Result<(), SoftwareCenterError> {
        let mut arguments = vec!["-y".to_owned(), operation.to_owned()];
        arguments.extend(extra.iter().map(|value| (*value).to_owned()));
        arguments.push("--".to_owned());
        arguments.push(package.as_str().to_owned());
        let result = self.command.run("/usr/bin/apt-get", &arguments).await?;
        if result.success {
            Ok(())
        } else {
            Err(SoftwareCenterError::Package)
        }
    }
}

#[async_trait]
impl PackageManager for AptPackageManager {
    async fn discover(&self) -> Result<HostSnapshot, SoftwareCenterError> {
        let release = tokio::fs::read_to_string("/etc/os-release")
            .await
            .map_err(|_| SoftwareCenterError::Package)?;
        let distribution = os_release_value(&release, "ID")?;
        let version = os_release_value(&release, "VERSION_ID")?;
        if !matches!(
            (distribution.as_str(), version.as_str()),
            ("ubuntu", "22.04" | "24.04") | ("debian", "12")
        ) {
            return Err(SoftwareCenterError::Invalid);
        }
        let architecture = match std::env::consts::ARCH {
            "x86_64" => "x86_64",
            "aarch64" => "aarch64",
            _ => return Err(SoftwareCenterError::Invalid),
        };
        let platform = SupportedPlatform::new(distribution, version, architecture)
            .map_err(|_| SoftwareCenterError::Invalid)?;
        let arguments = vec!["-W".to_owned(), "-f=${Package}\t${Version}\n".to_owned()];
        let packages = self.command.run("/usr/bin/dpkg-query", &arguments).await?;
        if !packages.success {
            return Err(SoftwareCenterError::Package);
        }
        Ok(HostSnapshot {
            platform,
            state_digest: hex::encode(Sha256::digest(packages.raw_output.as_bytes())),
            installed_packages: packages
                .raw_output
                .lines()
                .filter_map(|line| line.split_once('\t').map(|(name, _)| name.to_owned()))
                .filter(|name| PackageId::new(name).is_ok())
                .collect::<BTreeSet<_>>(),
        })
    }

    async fn apply(&self, actions: &[PlanAction]) -> Result<(), SoftwareCenterError> {
        for action in actions {
            match action {
                PlanAction::Install(package) => {
                    self.package_action("install", package, &[]).await?
                }
                PlanAction::Update(package) => {
                    self.package_action("install", package, &["--only-upgrade"])
                        .await?
                }
                PlanAction::Remove(package) => self.package_action("remove", package, &[]).await?,
            }
        }
        Ok(())
    }

    async fn validate(&self, component: &str) -> Result<(), SoftwareCenterError> {
        let (program, arguments): (&'static str, Vec<String>) = match component {
            "nginx" => ("/usr/sbin/nginx", vec!["-t".into()]),
            "php-8.3" => ("/usr/sbin/php-fpm8.3", vec!["-t".into()]),
            "php-8.4" => ("/usr/sbin/php-fpm8.4", vec!["-t".into()]),
            "mysql" | "mariadb" => ("/usr/bin/mysqladmin", vec!["ping".into()]),
            "redis" => ("/usr/bin/redis-cli", vec!["ping".into()]),
            _ => return Err(SoftwareCenterError::Invalid),
        };
        let result = self.command.run(program, &arguments).await?;
        if result.success {
            Ok(())
        } else {
            Err(SoftwareCenterError::Validation)
        }
    }

    async fn rollback(&self, actions: &[PlanAction]) -> Result<(), SoftwareCenterError> {
        for action in actions.iter().rev() {
            match action {
                PlanAction::Install(package) => self.package_action("remove", package, &[]).await?,
                PlanAction::Update(_) | PlanAction::Remove(_) => {
                    return Err(SoftwareCenterError::Package);
                }
            }
        }
        Ok(())
    }
}

fn os_release_value(input: &str, key: &str) -> Result<String, SoftwareCenterError> {
    let prefix = format!("{key}=");
    input
        .lines()
        .find_map(|line| line.strip_prefix(&prefix))
        .map(|value| value.trim_matches('"').to_ascii_lowercase())
        .filter(|value| !value.is_empty())
        .ok_or(SoftwareCenterError::Invalid)
}
