//! Fixed-argument APT adapter for explicitly supported Debian-family hosts.

use std::{collections::BTreeSet, sync::Arc};

use async_trait::async_trait;
use openpanel_domain::software_center::{PackageId, PlanAction, SupportedPlatform};
use sha2::{Digest, Sha256};
use tokio::process::Command;

use super::{HostSnapshot, PackageManager, SoftwareCenterError, host_package_manager::first_lines};

const OUTPUT_LIMIT: usize = 8_192;

/// Bounded command result with no command line or environment attached.
#[derive(Debug, Clone)]
pub struct CommandResult {
    /// Whether the fixed command completed successfully.
    pub success: bool,
    /// Bounded, control-character-sanitized diagnostic output.
    pub output: String,
    pub(crate) raw_output: String,
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

    /// Construct a failed deterministic result.
    pub fn failure(output: &str) -> Self {
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
            .map_err(|_| SoftwareCenterError::Package("operation failed".into()))?;
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

/// Run a `PackageCommand` as the current user or, if the current
/// process is not root, through `sudo -n` (non-interactive) so the
/// operator can keep the panel unprivileged. Mirrors the pattern used
/// by Baota (宝塔) and other production server panels: a dedicated
/// `openpanel` user gets passwordless sudo over a tightly scoped
/// allowlist of package binaries, and the panel never holds a root
/// shell. When the allowlist is missing, the wrapper fails with the
/// exact `/etc/sudoers.d/openpanel` entry the operator must install.
pub struct PrivilegedCommand {
    inner: Arc<dyn PackageCommand>,
    is_root: Box<dyn Fn() -> bool + Send + Sync>,
}

impl PrivilegedCommand {
    /// Wrap `inner`. Root detection reads `/proc/self/status`; the
    /// test seam replaces it via [`PrivilegedCommand::with_root_detector`].
    pub fn new(inner: Arc<dyn PackageCommand>) -> Self {
        Self {
            inner,
            is_root: Box::new(detect_root_from_proc),
        }
    }

    /// Replace the root detector. Used by the test suite to drive
    /// both branches without running the test as a different user.
    pub fn with_root_detector(mut self, is_root: Box<dyn Fn() -> bool + Send + Sync>) -> Self {
        self.is_root = is_root;
        self
    }

    /// Construct from a runtime check. Exposed for callers that want
    /// to decide separately whether to elevate (e.g. a per-binary
    /// policy that the wrapper cannot derive from the program name).
    pub fn for_program(
        program: &'static str,
        inner: Arc<dyn PackageCommand>,
    ) -> Arc<dyn PackageCommand> {
        if needs_privilege(program) && !privileged_root() {
            Arc::new(Self::new(inner))
        } else {
            inner
        }
    }
}

#[async_trait]
impl PackageCommand for PrivilegedCommand {
    async fn run(
        &self,
        program: &'static str,
        arguments: &[String],
    ) -> Result<CommandResult, SoftwareCenterError> {
        if !needs_privilege(program) || (self.is_root)() {
            return self.inner.run(program, arguments).await;
        }
        let mut wrapped = vec![program.to_owned()];
        wrapped.extend(arguments.iter().cloned());
        let sudo = sudo_binary();
        let result = self.inner.run(sudo, &wrapped).await.map_err(|error| {
            SoftwareCenterError::Package(format!("failed to spawn {sudo}: {error}"))
        })?;
        if result.success {
            return Ok(result);
        }
        Err(SoftwareCenterError::Package(format!(
            "{sudo} {program} (and its arguments) failed: {}. \
             The panel is not running as root and `sudo -n` (non-interactive) \
             rejected the request. Add the following line to \
             /etc/sudoers.d/openpanel on this host and reload sudo \
             (visudo -c && systemctl restart sudo):\n\
             \n\
             openpanel ALL=(root) NOPASSWD: {}\n\
             \n\
             Or run the panel as the root user (not recommended).",
            first_lines(&result.output, 4),
            sudoers_allowlist()
        )))
    }
}

/// Programs that must run with elevated privilege to install, remove,
/// or update packages. The snapshot/list side uses different binaries
/// (e.g. `/usr/bin/dpkg-query` on apt) that do not need root, so we
/// skip `sudo` for those.
pub fn needs_privilege(program: &str) -> bool {
    matches!(
        program,
        "/usr/bin/apt-get"
            | "/usr/bin/dnf"
            | "/usr/bin/yum"
            | "/usr/bin/zypper"
            | "/usr/bin/pacman"
            | "/sbin/apk"
    )
}

const fn sudo_binary() -> &'static str {
    "/usr/bin/sudo"
}

fn privileged_root() -> bool {
    detect_root_from_proc()
}

fn detect_root_from_proc() -> bool {
    let Ok(content) = std::fs::read_to_string("/proc/self/status") else {
        return false;
    };
    for line in content.lines() {
        if let Some(rest) = line.strip_prefix("Uid:") {
            let mut fields = rest.split_whitespace();
            let Some(effective) = fields.next() else {
                continue;
            };
            return effective == "0";
        }
    }
    false
}

fn sudoers_allowlist() -> String {
    [
        "/usr/bin/apt-get",
        "/usr/bin/dnf",
        "/usr/bin/yum",
        "/usr/bin/zypper",
        "/usr/bin/pacman",
        "/sbin/apk",
    ]
    .join(", ")
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
            Err(SoftwareCenterError::Package(format!(
                "/usr/bin/apt-get {operation} {} failed: {}",
                package.as_str(),
                first_lines(&result.output, 8)
            )))
        }
    }
}

#[async_trait]
impl PackageManager for AptPackageManager {
    async fn discover(&self) -> Result<HostSnapshot, SoftwareCenterError> {
        let release = tokio::fs::read_to_string("/etc/os-release")
            .await
            .map_err(|_| SoftwareCenterError::Package("operation failed".into()))?;
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
            return Err(SoftwareCenterError::Package(format!(
                "dpkg-query list failed: {}",
                first_lines(&packages.output, 4)
            )));
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
                    return Err(SoftwareCenterError::Package(
                        "cannot roll back an update or remove".into(),
                    ));
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
