//! Non-PHP runtime bounded context: per-site runtime choice
//! (Node / Python / Go / Ruby / .NET), pinned version, app port,
//! supervisor unit, and the nginx reverse-proxy block that
//! targets `127.0.0.1:APP_PORT`.
//!
//! Every check is allow-listed: the runtime kind, the version pin,
//! the app port, and the working directory must all be safe.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::RepoError;

/// Errors raised by the app-runtime bounded context.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RuntimeError {
    /// The caller is not authorised.
    #[error("forbidden")]
    Forbidden,
    /// The runtime kind is not allowed.
    #[error("runtime kind not allowed: {0}")]
    KindNotAllowed(String),
    /// The version pin is not allowed.
    #[error("version pin not allowed: {0}")]
    VersionNotAllowed(String),
    /// The app port is not allowed (not in 1024..=65535 or
    /// restricted).
    #[error("app port not allowed: {0}")]
    PortNotAllowed(u16),
    /// The working directory escapes the site chroot.
    #[error("working directory outside chroot: {0}")]
    OutsideChroot(String),
    /// The runtime binding address is not loopback.
    #[error("bind address must be loopback: {0}")]
    BindNotLoopback(String),
    /// Persistence failed.
    #[error("persistence failed: {0}")]
    Persistence(String),
}

impl From<RuntimeError> for RepoError {
    fn from(error: RuntimeError) -> Self {
        RepoError::new(error.to_string())
    }
}

impl From<RepoError> for RuntimeError {
    fn from(error: RepoError) -> Self {
        RuntimeError::Persistence(error.0)
    }
}

/// Runtime family.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeKind {
    /// Node.js.
    Node,
    /// Python.
    Python,
    /// Go.
    Go,
    /// Ruby.
    Ruby,
    /// .NET.
    Dotnet,
}

impl RuntimeKind {
    /// Stable lower-case label.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Node => "node",
            Self::Python => "python",
            Self::Go => "go",
            Self::Ruby => "ruby",
            Self::Dotnet => "dotnet",
        }
    }
}

/// Status of the runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeStatus {
    /// Supervisor unit is configured but not running.
    Stopped,
    /// Supervisor unit is running.
    Running,
    /// Supervisor unit failed.
    Failed,
}

impl RuntimeStatus {
    /// Stable lower-case label.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Stopped => "stopped",
            Self::Running => "running",
            Self::Failed => "failed",
        }
    }
}

/// A per-site runtime configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SiteRuntime {
    /// Stable id.
    pub id: Uuid,
    /// Owning site.
    pub site_id: Uuid,
    /// Runtime family.
    pub kind: RuntimeKind,
    /// Pinned version (e.g. `20.10.0` for Node 20).
    pub version: String,
    /// Port the app binds to (loopback only).
    pub app_port: u16,
    /// Working directory inside the site chroot.
    pub workdir: String,
    /// Start command (shell-style; sandboxed at execution).
    #[serde(default)]
    pub start_command: String,
    /// When the runtime was registered.
    pub registered_at: DateTime<Utc>,
    /// Current status.
    pub status: RuntimeStatus,
}

impl SiteRuntime {
    /// Validate the runtime's invariants.
    pub fn validate(&self) -> Result<(), RuntimeError> {
        if !is_kind_allowed(self.kind) {
            return Err(RuntimeError::KindNotAllowed(self.kind.as_str().into()));
        }
        if !is_version_allowed(&self.version) {
            return Err(RuntimeError::VersionNotAllowed(self.version.clone()));
        }
        if !is_port_allowed(self.app_port) {
            return Err(RuntimeError::PortNotAllowed(self.app_port));
        }
        if !is_workdir_inside_chroot(&self.workdir) {
            return Err(RuntimeError::OutsideChroot(self.workdir.clone()));
        }
        Ok(())
    }
}

/// Default allow-list of runtime kinds. New kinds MUST be added
/// here; the validator refuses everything else.
pub const ALLOWED_RUNTIME_KINDS: &[RuntimeKind] = &[
    RuntimeKind::Node,
    RuntimeKind::Python,
    RuntimeKind::Go,
    RuntimeKind::Ruby,
    RuntimeKind::Dotnet,
];

/// Default allow-list of version pins. The validator accepts
/// any MAJOR.MINOR.PATCH-style string with exactly three numeric
/// parts in the 1..=255 range. Production wiring swaps in a
/// narrower allow-list per LTS policy.
pub fn is_kind_allowed(kind: RuntimeKind) -> bool {
    ALLOWED_RUNTIME_KINDS.contains(&kind)
}

/// Parse `version` as `MAJOR.MINOR.PATCH` and return true when
/// the parts are valid.
pub fn is_version_allowed(version: &str) -> bool {
    let mut parts = version.split('.');
    let major = match parts.next().and_then(|p| p.parse::<u32>().ok()) {
        Some(v) if (1..=255).contains(&v) => v,
        _ => return false,
    };
    let minor = match parts.next().and_then(|p| p.parse::<u32>().ok()) {
        Some(v) if v <= 255 => v,
        _ => return false,
    };
    let patch = match parts.next().and_then(|p| p.parse::<u32>().ok()) {
        Some(v) if v <= 255 => v,
        _ => return false,
    };
    if parts.next().is_some() {
        return false;
    }
    let _ = (major, minor, patch);
    true
}

/// Default port allow-list. We require 1024..=65535 and reject
/// well-known privileged ports.
pub fn is_port_allowed(port: u16) -> bool {
    port >= 1024 && port <= 65535 && !RESERVED_PORTS.contains(&port)
}

/// Reserved ports the runtime may not bind to.
pub const RESERVED_PORTS: &[u16] = &[
    22, 25, 80, 443, 3306, 5432, 6379, 27017, 9080,
];

/// Reject paths that escape the chroot (`..`, absolute paths,
/// or paths with embedded null).
pub fn is_workdir_inside_chroot(workdir: &str) -> bool {
    if workdir.is_empty() || workdir.contains("..") || workdir.starts_with('/') {
        return false;
    }
    if workdir.bytes().any(|b| b == 0) {
        return false;
    }
    true
}

/// Render the supervisor unit for the runtime. The unit runs
/// the app as the site user, with `WorkingDirectory` rooted in
/// the site chroot.
pub fn render_supervisor_unit(runtime: &SiteRuntime, site_user: &str, home: &str) -> String {
    format!(
        r#"[Unit]
Description=OpenPanel site runtime {site} ({kind})
After=network.target

[Service]
Type=simple
User={user}
WorkingDirectory={home}/{workdir}
ExecStart={start}
Restart=on-failure
RestartSec=5
Environment=APP_PORT={port}

[Install]
WantedBy=multi-user.target
"#,
        site = runtime.site_id,
        kind = runtime.kind.as_str(),
        user = site_user,
        home = home,
        workdir = runtime.workdir,
        start = if runtime.start_command.is_empty() {
            "/usr/bin/env -S /bin/sh -c 'exec app'"
        } else {
            runtime.start_command.as_str()
        },
        port = runtime.app_port,
    )
}

/// Render the nginx `proxy_pass` block for the runtime. The
/// proxy only targets loopback; the response header set keeps
/// the app behind the panel's TLS terminator.
pub fn render_nginx_proxy_block(runtime: &SiteRuntime, server_name: &str) -> String {
    format!(
        r#"server {{
    listen 80;
    listen [::]:80;
    server_name {server_name};

    location / {{
        proxy_pass http://127.0.0.1:{port};
        proxy_http_version 1.1;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }}
}}
"#,
        server_name = server_name,
        port = runtime.app_port,
    )
}

/// Persistence port for the app-runtime bounded context.
#[async_trait]
pub trait RuntimeRepository: Send + Sync + 'static {
    /// Persist a runtime.
    async fn save_runtime(&self, runtime: &SiteRuntime) -> Result<(), RepoError>;
    /// Load a runtime by site id.
    async fn get_runtime(&self, site_id: Uuid) -> Result<Option<SiteRuntime>, RepoError>;
    /// List all runtimes.
    async fn list_runtimes(&self) -> Result<Vec<SiteRuntime>, RepoError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok_runtime() -> SiteRuntime {
        SiteRuntime {
            id: Uuid::new_v4(),
            site_id: Uuid::new_v4(),
            kind: RuntimeKind::Node,
            version: "20.10.0".into(),
            app_port: 3000,
            workdir: "app".into(),
            start_command: String::new(),
            registered_at: Utc::now(),
            status: RuntimeStatus::Stopped,
        }
    }

    #[test]
    fn validate_accepts_a_safe_runtime() {
        let runtime = ok_runtime();
        assert!(runtime.validate().is_ok());
    }

    #[test]
    fn validate_rejects_absolute_workdir() {
        let mut runtime = ok_runtime();
        runtime.workdir = "/etc/passwd".into();
        assert!(matches!(runtime.validate(), Err(RuntimeError::OutsideChroot(_))));
    }

    #[test]
    fn validate_rejects_parent_traversal() {
        let mut runtime = ok_runtime();
        runtime.workdir = "../etc".into();
        assert!(matches!(runtime.validate(), Err(RuntimeError::OutsideChroot(_))));
    }

    #[test]
    fn validate_rejects_privileged_port() {
        let mut runtime = ok_runtime();
        runtime.app_port = 80;
        assert!(matches!(runtime.validate(), Err(RuntimeError::PortNotAllowed(_))));
    }

    #[test]
    fn validate_rejects_bad_version() {
        let mut runtime = ok_runtime();
        runtime.version = "not-a-version".into();
        assert!(matches!(runtime.validate(), Err(RuntimeError::VersionNotAllowed(_))));
    }

    #[test]
    fn version_allows_three_part_semver() {
        assert!(is_version_allowed("20.10.0"));
        assert!(is_version_allowed("1.0.0"));
        assert!(!is_version_allowed("1.0"));
        assert!(!is_version_allowed("1.0.0.0"));
        assert!(!is_version_allowed("0.0.0"));
    }

    #[test]
    fn nginx_block_targets_loopback_only() {
        let runtime = ok_runtime();
        let block = render_nginx_proxy_block(&runtime, "example.com");
        assert!(block.contains("proxy_pass http://127.0.0.1:3000"));
        assert!(!block.contains("0.0.0.0"));
    }

    #[test]
    fn supervisor_unit_uses_chroot_workdir() {
        let runtime = ok_runtime();
        let unit = render_supervisor_unit(&runtime, "site-user", "/srv/site");
        assert!(unit.contains("WorkingDirectory=/srv/site/app"));
        assert!(unit.contains("User=site-user"));
    }
}