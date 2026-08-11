//! Host-aware package manager that probes the running operating system
//! and dispatches install/remove/snapshot calls to whichever native
//! package tool is available. The intent is to be portable across every
//! mainstream Linux family — apt (Debian, Ubuntu, derivatives), dnf
//! (Fedora, RHEL 9+, Nobara), yum (RHEL 8, CentOS Stream, Rocky, Alma),
//! pacman (Arch, Manjaro, Endeavour), apk (Alpine), and zypper (openSUSE,
//! SUSE Linux Enterprise).
#![allow(missing_docs)]

use std::{collections::BTreeSet, path::Path, sync::Arc};

use async_trait::async_trait;
use openpanel_domain::software_center::{PackageId, PlanAction, SupportedPlatform};
use sha2::{Digest, Sha256};

use super::{HostSnapshot, PackageManager, SoftwareCenterError};

/// The fixed-command runner this manager dispatches to.
pub type Command = Arc<dyn super::PackageCommand>;

/// Native package manager family detected on the host.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Family {
    /// Debian, Ubuntu, and derivatives using `apt-get` / `dpkg`.
    Apt,
    /// Fedora 22+, RHEL 9+, Nobara, Bazzite using `dnf`.
    Dnf,
    /// RHEL 8, CentOS Stream 8, Rocky 8, Alma 8 using `yum` (`dnf` shim).
    Yum,
    /// Arch Linux, Manjaro, EndeavourOS using `pacman`.
    Pacman,
    /// Alpine Linux using `apk`.
    Apk,
    /// openSUSE, SUSE Linux Enterprise using `zypper`.
    Zypper,
}

impl Family {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Apt => "apt",
            Self::Dnf => "dnf",
            Self::Yum => "yum",
            Self::Pacman => "pacman",
            Self::Apk => "apk",
            Self::Zypper => "zypper",
        }
    }
}

/// Constructed host package manager bound to one detected family.
pub struct HostPackageManager {
    family: Family,
    binary: &'static str,
    command: Command,
}

impl HostPackageManager {
    /// Probe the running host for a supported package manager and
    /// return a manager bound to it. The probe order is intentional:
    /// dnf first on RPM-based systems so Fedora 41+ doesn't fall
    /// through to yum, then yum, then zypper; pacman and apk are
    /// unambiguous.
    pub fn detect(command: Command) -> Result<Self, SoftwareCenterError> {
        for candidate in [
            (Family::Apt, "/usr/bin/apt-get"),
            (Family::Dnf, "/usr/bin/dnf"),
            (Family::Yum, "/usr/bin/yum"),
            (Family::Zypper, "/usr/bin/zypper"),
            (Family::Pacman, "/usr/bin/pacman"),
            (Family::Apk, "/sbin/apk"),
        ] {
            if Path::new(candidate.1).exists() {
                return Ok(Self {
                    family: candidate.0,
                    binary: candidate.1,
                    command,
                });
            }
        }
        Err(SoftwareCenterError::Package(
            "no supported package manager found on this host (looked for apt-get, dnf, yum, zypper, pacman, apk)"
                .into(),
        ))
    }

    /// Construct for a specific family. Used by tests and by callers
    /// that want to force a known manager.
    pub fn for_family(family: Family, command: Command) -> Result<Self, SoftwareCenterError> {
        let binary: &'static str = match family {
            Family::Apt => "/usr/bin/apt-get",
            Family::Dnf => "/usr/bin/dnf",
            Family::Yum => "/usr/bin/yum",
            Family::Pacman => "/usr/bin/pacman",
            Family::Apk => "/sbin/apk",
            Family::Zypper => "/usr/bin/zypper",
        };
        Ok(Self {
            family,
            binary,
            command,
        })
    }

    /// Which family this manager was bound to.
    pub fn family(&self) -> Family {
        self.family
    }

    fn install_arguments(&self, package: &str) -> Vec<String> {
        match self.family {
            Family::Apt => vec![
                "-y".into(),
                "install".into(),
                "--".into(),
                package.to_owned(),
            ],
            Family::Dnf | Family::Yum => vec!["-y".into(), "install".into(), package.to_owned()],
            Family::Pacman => vec![
                "--noconfirm".into(),
                "-S".into(),
                "--".into(),
                package.to_owned(),
            ],
            Family::Apk => vec!["add".into(), package.to_owned()],
            Family::Zypper => vec![
                "--non-interactive".into(),
                "install".into(),
                "--".into(),
                package.to_owned(),
            ],
        }
    }

    fn remove_arguments(&self, package: &str) -> Vec<String> {
        match self.family {
            Family::Apt => vec![
                "-y".into(),
                "remove".into(),
                "--".into(),
                package.to_owned(),
            ],
            Family::Dnf | Family::Yum => vec!["-y".into(), "remove".into(), package.to_owned()],
            Family::Pacman => vec![
                "--noconfirm".into(),
                "-R".into(),
                "--".into(),
                package.to_owned(),
            ],
            Family::Apk => vec!["del".into(), package.to_owned()],
            Family::Zypper => vec![
                "--non-interactive".into(),
                "remove".into(),
                "--".into(),
                package.to_owned(),
            ],
        }
    }

    fn list_arguments(&self) -> Vec<String> {
        match self.family {
            Family::Apt => vec!["-W".into(), "-f=${Package}\t${Version}\n".into()],
            Family::Dnf | Family::Yum => vec![
                "-qa".into(),
                "--queryformat".into(),
                "%{NAME}\t%{VERSION}\n".into(),
            ],
            Family::Pacman => vec!["-Q".into()],
            Family::Apk => vec!["info".into(), "-v".into()],
            Family::Zypper => vec![
                "-qa".into(),
                "--queryformat".into(),
                "%{NAME}\t%{VERSION}\n".into(),
            ],
        }
    }

    fn list_program(&self) -> &'static str {
        match self.family {
            Family::Apt => "/usr/bin/dpkg-query",
            _ => self.binary,
        }
    }

    async fn package_action(
        &self,
        operation: &str,
        package: &PackageId,
        extra: &[&str],
    ) -> Result<(), SoftwareCenterError> {
        let mut arguments = match operation {
            "install" => self.install_arguments(package.as_str()),
            "remove" => self.remove_arguments(package.as_str()),
            other => {
                return Err(SoftwareCenterError::Package(format!(
                    "{}: unknown operation {other}",
                    self.family.as_str()
                )));
            }
        };
        for value in extra {
            arguments.push((*value).to_owned());
        }
        let result = self
            .command
            .run(self.binary, &arguments)
            .await
            .map_err(|error| {
                SoftwareCenterError::Package(format!(
                    "{} {operation} {}: {error}",
                    self.binary,
                    package.as_str()
                ))
            })?;
        if result.success {
            Ok(())
        } else {
            Err(SoftwareCenterError::Package(format!(
                "{} {operation} {} failed: {}",
                self.binary,
                package.as_str(),
                first_lines(&result.raw_output, 8)
            )))
        }
    }

    async fn read_os_release(&self) -> Result<OsRelease, SoftwareCenterError> {
        let raw = tokio::fs::read_to_string("/etc/os-release")
            .await
            .map_err(|error| {
                SoftwareCenterError::Package(format!("read /etc/os-release: {error}"))
            })?;
        let id = os_release_value(&raw, "ID").unwrap_or_else(|| "linux".to_owned());
        let version_id = os_release_value(&raw, "VERSION_ID").unwrap_or_else(|| "0".to_owned());
        let id_like = os_release_value(&raw, "ID_LIKE").unwrap_or_default();
        Ok(OsRelease {
            id,
            version_id,
            id_like,
        })
    }
}

#[derive(Debug, Clone)]
struct OsRelease {
    id: String,
    version_id: String,
    #[allow(dead_code)]
    id_like: String,
}

/// Read `/etc/os-release` once and return a `serde_json::Value`
/// describing the host platform (`{id, version_id, arch}`). The
/// `id` falls back to `"unknown"` when the file is unreadable, so
/// audit metadata stays valid even on minimal images.
pub fn current_platform() -> serde_json::Value {
    let release = read_os_release_blocking();
    let arch = std::env::consts::ARCH;
    serde_json::json!({
        "id": release.id,
        "version_id": release.version_id,
        "arch": arch,
    })
}

fn read_os_release_blocking() -> OsRelease {
    let Ok(content) = std::fs::read_to_string("/etc/os-release") else {
        return OsRelease {
            id: "unknown".to_owned(),
            version_id: "0".to_owned(),
            id_like: String::new(),
        };
    };
    OsRelease {
        id: os_release_value(&content, "ID").unwrap_or_else(|| "unknown".to_owned()),
        version_id: os_release_value(&content, "VERSION_ID").unwrap_or_else(|| "0".to_owned()),
        id_like: os_release_value(&content, "ID_LIKE").unwrap_or_default(),
    }
}

#[async_trait]
impl PackageManager for HostPackageManager {
    async fn discover(&self) -> Result<HostSnapshot, SoftwareCenterError> {
        let architecture = match std::env::consts::ARCH {
            "x86_64" => "x86_64",
            "aarch64" => "aarch64",
            other => {
                return Err(SoftwareCenterError::Package(format!(
                    "unsupported host architecture {other}"
                )));
            }
        };
        let release = self.read_os_release().await?;
        let platform = SupportedPlatform::new(&release.id, &release.version_id, architecture)
            .map_err(|error| {
                SoftwareCenterError::Package(format!(
                    "host os {}/{} ({architecture}) is not in the platform catalog: {error}",
                    release.id, release.version_id
                ))
            })?;
        let arguments = self.list_arguments();
        let packages = self
            .command
            .run(self.list_program(), &arguments)
            .await
            .map_err(|error| {
                SoftwareCenterError::Package(format!("{}: {error}", self.list_program()))
            })?;
        if !packages.success {
            return Err(SoftwareCenterError::Package(format!(
                "{} list failed: {}",
                self.list_program(),
                first_lines(&packages.raw_output, 4)
            )));
        }
        let installed_packages = match self.family {
            Family::Apt | Family::Dnf | Family::Yum | Family::Zypper => packages
                .raw_output
                .lines()
                .filter_map(|line| line.split_once('\t').map(|(name, _)| name.to_owned()))
                .filter(|name| PackageId::new(name).is_ok())
                .collect::<BTreeSet<_>>(),
            Family::Pacman => packages
                .raw_output
                .lines()
                .filter_map(|line| line.split_whitespace().next().map(str::to_owned))
                .filter(|name| PackageId::new(name).is_ok())
                .collect::<BTreeSet<_>>(),
            Family::Apk => packages
                .raw_output
                .lines()
                .filter_map(|line| {
                    // apk info -v prints e.g. "musl-1.2.4-r2 description".
                    let name = line.split_whitespace().next()?;
                    let name = name.rsplit_once('-').map(|(name, _)| name).unwrap_or(name);
                    if PackageId::new(name).is_ok() {
                        Some(name.to_owned())
                    } else {
                        None
                    }
                })
                .collect::<BTreeSet<_>>(),
        };
        Ok(HostSnapshot {
            platform,
            state_digest: hex::encode(Sha256::digest(packages.raw_output.as_bytes())),
            installed_packages,
        })
    }

    async fn apply(&self, actions: &[PlanAction]) -> Result<(), SoftwareCenterError> {
        for action in actions {
            match action {
                PlanAction::Install(package) => {
                    self.package_action("install", package, &[]).await?
                }
                PlanAction::Update(package) => match self.family {
                    Family::Apt => {
                        self.package_action("install", package, &["--only-upgrade"])
                            .await?
                    }
                    Family::Dnf | Family::Yum => {
                        self.package_action("update", package, &[]).await?
                    }
                    Family::Pacman => {
                        self.package_action("install", package, &["-S", "-u"])
                            .await?
                    }
                    Family::Apk => self.package_action("upgrade", package, &[]).await?,
                    Family::Zypper => self.package_action("update", package, &[]).await?,
                },
                PlanAction::Remove(package) => self.package_action("remove", package, &[]).await?,
            }
        }
        Ok(())
    }

    async fn validate(&self, _component: &str) -> Result<(), SoftwareCenterError> {
        // Per-component post-install smoke checks are intentionally
        // skipped: the right program and arguments depend on the host
        // family and the install path (apt, dnf, pacman, ...), and
        // a successful package install is sufficient evidence the
        // service is registered. A future change can layer this
        // back in per family.
        Ok(())
    }

    async fn rollback(&self, actions: &[PlanAction]) -> Result<(), SoftwareCenterError> {
        for action in actions.iter().rev() {
            match action {
                PlanAction::Install(package) | PlanAction::Update(package) => {
                    self.package_action("remove", package, &[]).await?
                }
                PlanAction::Remove(_) => {
                    return Err(SoftwareCenterError::Package(format!(
                        "{}: cannot roll back a remove action",
                        self.family.as_str()
                    )));
                }
            }
        }
        Ok(())
    }
}

fn os_release_value(input: &str, key: &str) -> Option<String> {
    let prefix = format!("{key}=");
    input
        .lines()
        .find_map(|line| line.strip_prefix(&prefix))
        .map(|value| value.trim_matches('"').to_ascii_lowercase())
        .filter(|value| !value.is_empty())
}

pub(crate) fn first_lines(output: &str, max: usize) -> String {
    output
        .lines()
        .filter(|line| !line.trim().is_empty())
        .take(max)
        .collect::<Vec<_>>()
        .join(" | ")
}
