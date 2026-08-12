//! Curated, server-owned configuration editor for panel-managed System
//! components.
//!
//! The manifest in this module is deliberately hard-coded: it is the
//! allowlist that bounds what an Owner can read and write. Catalog data,
//! environment variables, and client input can never add a path here.

use std::path::{Path, PathBuf};

use openpanel_domain::Role;
use serde::Serialize;
use tokio::fs;

use super::{SoftwareCenterError, SoftwareCenterService};

/// Read/write ceiling for a config file, in bytes (128 KiB).
pub const MAX_CONFIG_BYTES: u64 = 128 * 1024;

/// One manifest entry: which component, its key config file, and whether
/// a validation pass may run after a save.
pub struct ComponentConfig {
    /// Catalog id, e.g. `nginx`.
    pub component: &'static str,
    /// Human label for the key file, e.g. `nginx.conf`.
    pub label: &'static str,
    /// File paths relative to the service config root. `files[0]` is the
    /// editable key file in this phase; the rest are reserved for later
    /// phases and are currently unused.
    pub files: &'static [&'static str],
    /// Whether the package adapter's `validate(component)` pass may run
    /// after a save.
    pub validatable: bool,
}

const NGINX: ComponentConfig = ComponentConfig {
    component: "nginx",
    label: "nginx.conf",
    files: &["etc/nginx/nginx.conf"],
    validatable: true,
};

const FAIL2BAN: ComponentConfig = ComponentConfig {
    component: "fail2ban",
    label: "jail.local",
    files: &["etc/fail2ban/jail.local"],
    validatable: true,
};

const PHP_80: ComponentConfig = ComponentConfig {
    component: "php-8.0",
    label: "php.ini (fpm)",
    files: &["etc/php/8.0/fpm/php.ini"],
    validatable: true,
};

const PHP_81: ComponentConfig = ComponentConfig {
    component: "php-8.1",
    label: "php.ini (fpm)",
    files: &["etc/php/8.1/fpm/php.ini"],
    validatable: true,
};

const PHP_82: ComponentConfig = ComponentConfig {
    component: "php-8.2",
    label: "php.ini (fpm)",
    files: &["etc/php/8.2/fpm/php.ini"],
    validatable: true,
};

const PHP_83: ComponentConfig = ComponentConfig {
    component: "php-8.3",
    label: "php.ini (fpm)",
    files: &["etc/php/8.3/fpm/php.ini"],
    validatable: true,
};

const MYSQL: ComponentConfig = ComponentConfig {
    component: "mysql",
    label: "mysqld.cnf",
    files: &["etc/mysql/mysql.conf.d/mysqld.cnf"],
    validatable: true,
};

const MARIADB: ComponentConfig = ComponentConfig {
    component: "mariadb",
    label: "50-server.cnf",
    files: &["etc/mysql/mariadb.conf.d/50-server.cnf"],
    validatable: true,
};

const REDIS: ComponentConfig = ComponentConfig {
    component: "redis",
    label: "redis.conf",
    files: &["etc/redis/redis.conf"],
    validatable: true,
};

static COMPONENT_CONFIGS: &[ComponentConfig] = &[
    NGINX, FAIL2BAN, PHP_80, PHP_81, PHP_82, PHP_83, MYSQL, MARIADB, REDIS,
];

/// Look up the manifest entry for a component id. Returns `None` when the
/// component has no server-owned configuration surface in this panel.
pub fn component_config(component: &str) -> Option<&'static ComponentConfig> {
    COMPONENT_CONFIGS
        .iter()
        .find(|cfg| cfg.component == component)
}

/// Metadata for the configuration page: resolved absolute path and whether
/// the file currently exists.
#[derive(Debug, Clone, Serialize)]
pub struct ComponentConfigInfo {
    /// Catalog id, e.g. `nginx`.
    pub component: &'static str,
    /// Human label for the key file.
    pub label: &'static str,
    /// Absolute path of the key config file.
    pub path: String,
    /// Whether the key config file exists on disk.
    pub exists: bool,
}

/// The config document returned by a read.
#[derive(Debug, Clone, Serialize)]
pub struct ComponentConfigDocument {
    /// Catalog id, e.g. `nginx`.
    pub component: &'static str,
    /// Human label for the key file.
    pub label: &'static str,
    /// Absolute path of the key config file.
    pub path: String,
    /// Whether the key config file exists on disk.
    pub exists: bool,
    /// Current file content, when the file exists. Bounded by
    /// [`MAX_CONFIG_BYTES`].
    pub content: Option<String>,
}

impl SoftwareCenterService {
    /// Metadata for the configuration page. Owner-only; requires a
    /// manifest entry and a panel-managed component.
    pub async fn component_config_info(
        &self,
        role: Role,
        component: &str,
    ) -> Result<ComponentConfigInfo, SoftwareCenterError> {
        let cfg = require_managed_config(self, component, role).await?;
        let path = config_path(&self.config_root, cfg);
        Ok(ComponentConfigInfo {
            component: cfg.component,
            label: cfg.label,
            exists: fs::try_exists(&path).await.map_err(io_error)?,
            path: path.display().to_string(),
        })
    }

    /// Read the current key config file. Owner-only; requires a manifest
    /// entry and a panel-managed component. A missing file is NOT an
    /// error: the response carries `exists: false` with no content.
    pub async fn read_config(
        &self,
        role: Role,
        component: &str,
    ) -> Result<ComponentConfigDocument, SoftwareCenterError> {
        let cfg = require_managed_config(self, component, role).await?;
        let path = config_path(&self.config_root, cfg);
        let (exists, content) = match fs::metadata(&path).await {
            Ok(meta) => {
                if meta.len() > MAX_CONFIG_BYTES {
                    return Err(SoftwareCenterError::Invalid(format!(
                        "config file {} exceeds the {MAX_CONFIG_BYTES} byte limit",
                        path.display()
                    )));
                }
                let content = fs::read_to_string(&path).await.map_err(io_error)?;
                (true, Some(content))
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (false, None),
            Err(error) => return Err(io_error(error)),
        };
        Ok(ComponentConfigDocument {
            component: cfg.component,
            label: cfg.label,
            exists,
            path: path.display().to_string(),
            content,
        })
    }

    /// Save the key config file atomically. Owner-only; requires a
    /// manifest entry and a panel-managed component. When the component
    /// is validatable and the post-write validation pass fails, the
    /// previous content is restored and the validation error is returned.
    /// Returns the absolute path written on success.
    pub async fn save_config(
        &self,
        role: Role,
        component: &str,
        content: String,
    ) -> Result<String, SoftwareCenterError> {
        let cfg = require_managed_config(self, component, role).await?;
        if content.len() > MAX_CONFIG_BYTES as usize {
            return Err(SoftwareCenterError::Invalid(format!(
                "config content exceeds the {MAX_CONFIG_BYTES} byte limit"
            )));
        }
        let path = config_path(&self.config_root, cfg);
        let previous = match fs::read_to_string(&path).await {
            Ok(value) => Some(value),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(io_error(error)),
        };
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await.map_err(io_error)?;
        }
        // Atomic-ish write: write to a sibling `.new` file, then rename
        // over the target, mirroring the sites module's discipline.
        let tmp = sibling_new(&path);
        fs::write(&tmp, &content).await.map_err(io_error)?;
        fs::rename(&tmp, &path).await.map_err(io_error)?;
        if cfg.validatable
            && let Err(error) = self.packages.validate(component).await
        {
            restore(&path, previous.as_deref()).await;
            return Err(error);
        }
        Ok(path.display().to_string())
    }
}

/// Gate the config surface: Owner-only, a manifest entry must exist, and
/// the component must be panel-managed.
async fn require_managed_config(
    service: &SoftwareCenterService,
    component: &str,
    role: Role,
) -> Result<&'static ComponentConfig, SoftwareCenterError> {
    if role != Role::Owner {
        return Err(SoftwareCenterError::Forbidden);
    }
    let cfg = component_config(component).ok_or(SoftwareCenterError::Unsupported)?;
    if !service.managed_components().await?.contains(component) {
        return Err(SoftwareCenterError::Forbidden);
    }
    Ok(cfg)
}

fn config_path(config_root: &Path, cfg: &ComponentConfig) -> PathBuf {
    config_root.join(cfg.files[0])
}

fn sibling_new(path: &Path) -> PathBuf {
    let mut name = path
        .file_name()
        .map(|name| name.to_os_string())
        .unwrap_or_default();
    name.push(".new");
    path.with_file_name(name)
}

async fn restore(path: &Path, previous: Option<&str>) {
    match previous {
        Some(value) => {
            let _ = fs::write(path, value).await;
        }
        None => {
            let _ = fs::remove_file(path).await;
        }
    }
}

fn io_error(error: std::io::Error) -> SoftwareCenterError {
    SoftwareCenterError::Package(format!("config file operation failed: {error}"))
}
