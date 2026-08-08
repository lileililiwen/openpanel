//! Config layering: built-in defaults → `/etc/openpanel/openpanel.toml` →
//! `$OPENPANEL_CONFIG` → `OPENPANEL__*` env vars → CLI overrides.
//!
//! Final config is validated against a built-in JSON Schema. On failure,
//! startup aborts with exit code 78 (`EX_CONFIG`).

use std::{path::Path, sync::Arc};

use figment::{
    Figment,
    providers::{Env, Format, Toml},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{ConfigError, CoreResult};

#[derive(Debug, Clone, Serialize, Deserialize)]
/// HTTP server settings.
pub struct ServerConfig {
    /// Address the server binds to.
    pub bind: String,
    /// Port the server listens on.
    pub port: u16,
    /// Optional number of worker threads (defaults when `None`).
    pub workers: Option<usize>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind: "0.0.0.0".to_string(),
            port: 8080,
            workers: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// Database connection settings.
pub struct DatabaseConfig {
    /// Driver name, e.g. `sqlite`.
    pub driver: String,
    /// Connection URL, e.g. `sqlite:///var/lib/openpanel/openpanel.db`.
    pub url: String,
    /// Maximum number of pooled connections.
    pub max_connections: u32,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            driver: "sqlite".to_string(),
            url: "/var/lib/openpanel/openpanel.db".to_string(),
            max_connections: 8,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// Logging settings.
pub struct LogConfig {
    /// Minimum log level, e.g. `info` or `debug`.
    pub level: String,
    /// Log output format, e.g. `compact` or `json`.
    pub format: String,
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            level: "info".to_string(),
            format: "compact".to_string(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
/// Validated application configuration assembled from defaults, files, and env.
pub struct Config {
    /// HTTP server settings.
    #[serde(default)]
    pub server: ServerConfig,
    /// Database connection settings.
    #[serde(default)]
    pub database: DatabaseConfig,
    /// Logging settings.
    #[serde(default)]
    pub log: LogConfig,
    /// Monitoring settings.
    #[serde(default)]
    pub monitoring: MonitoringConfig,
    /// Per-module config sections keyed by module name.
    #[serde(default)]
    pub modules: serde_json::Map<String, Value>,
}

/// Monitoring module settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitoringConfig {
    /// Seconds between collector ticks.
    pub interval_secs: u64,
    /// Retention window in days; `0` disables pruning.
    pub retention_days: u64,
    /// Optional alert thresholds, keyed by metric kind.
    #[serde(default)]
    pub alert: serde_json::Map<String, Value>,
}

impl Default for MonitoringConfig {
    fn default() -> Self {
        Self {
            interval_secs: 60,
            retention_days: 7,
            alert: serde_json::Map::new(),
        }
    }
}

impl Config {
    /// Load config from disk + env. CLI overrides are applied by the caller
    /// through `with_overrides`.
    pub fn load() -> CoreResult<Arc<Self>> {
        let defaults =
            toml::to_string(&Config::default()).map_err(|e| ConfigError::Load(e.to_string()))?;
        let mut figment = Figment::new().merge(Toml::string(&defaults));

        if path_exists("/etc/openpanel/openpanel.toml") {
            figment = figment.merge(Toml::file("/etc/openpanel/openpanel.toml").nested());
        }
        if let Ok(user_path) = std::env::var("OPENPANEL_CONFIG")
            && path_exists(&user_path)
        {
            figment = figment.merge(Toml::file(user_path).nested());
        }
        figment = figment.merge(Env::prefixed("OPENPANEL__").split("__"));

        let cfg: Config = figment
            .extract()
            .map_err(|e| ConfigError::Load(e.to_string()))?;

        cfg.validate()?;
        Ok(Arc::new(cfg))
    }

    /// Validate the config against the built-in JSON Schema.
    pub fn validate(&self) -> CoreResult<()> {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "server": {
                    "type": "object",
                    "properties": {
                        "bind": {"type": "string"},
                        "port": {"type": "integer", "minimum": 1, "maximum": 65535},
                    },
                    "required": ["bind", "port"],
                },
                "database": {
                    "type": "object",
                    "properties": {
                        "driver": {"type": "string", "enum": ["sqlite"]},
                        "url": {"type": "string", "minLength": 1},
                        "max_connections": {"type": "integer", "minimum": 1},
                    },
                    "required": ["driver", "url"],
                },
                "log": {
                    "type": "object",
                    "properties": {
                        "level": {"type": "string"},
                        "format": {"type": "string", "enum": ["compact", "json"]},
                    },
                },
                "monitoring": {
                    "type": "object",
                    "properties": {
                        "interval_secs": {"type": "integer", "minimum": 1},
                        "retention_days": {"type": "integer", "minimum": 0},
                        "alert": {"type": "object"},
                    },
                },
                "modules": {"type": "object"},
            },
            "required": ["server", "database"],
        });

        let validator =
            jsonschema::validator_for(&schema).map_err(|e| ConfigError::Schema(e.to_string()))?;
        let value = serde_json::to_value(self).map_err(|e| ConfigError::Load(e.to_string()))?;
        let mut errors_iter = validator.iter_errors(&value);
        if let Some(first) = errors_iter.next() {
            return Err(ConfigError::Validation {
                path: first.instance_path.to_string(),
                message: first.to_string(),
            }
            .into());
        }
        Ok(())
    }

    /// Accessor for the server settings.
    pub fn server(&self) -> &ServerConfig {
        &self.server
    }

    /// Accessor for the database settings.
    pub fn database(&self) -> &DatabaseConfig {
        &self.database
    }

    /// Accessor for the logging settings.
    pub fn log(&self) -> &LogConfig {
        &self.log
    }

    /// Accessor for the monitoring settings.
    pub fn monitoring(&self) -> &MonitoringConfig {
        &self.monitoring
    }

    /// Look up a module's config section, if present.
    pub fn module_config(&self, name: &str) -> Option<&Value> {
        self.modules.get(name)
    }
}

/// Apply a CLI override (`path = value`) into the config, returning the
/// updated config for validation.
pub fn with_overrides(mut cfg: Config, path: &str, value: Value) -> CoreResult<Config> {
    let segments: Vec<&str> = path.split('.').collect();
    let mut current = serde_json::to_value(&cfg).map_err(|e| ConfigError::Load(e.to_string()))?;
    let mut cursor: &mut Value = &mut current;
    for seg in &segments {
        if !cursor.is_object() {
            return Err(ConfigError::Load(format!("path `{path}` is not an object")).into());
        }
        let obj = cursor
            .as_object_mut()
            .ok_or_else(|| ConfigError::Load(format!("path `{path}` is not an object")))?;
        if !obj.contains_key(*seg) {
            obj.insert((*seg).to_string(), Value::Null);
        }
        cursor = obj.get_mut(*seg).ok_or_else(|| {
            ConfigError::Load(format!("segment `{seg}` vanished from path `{path}`"))
        })?;
    }
    *cursor = value;
    let updated: Config =
        serde_json::from_value(current).map_err(|e| ConfigError::Load(e.to_string()))?;
    cfg = updated;
    Ok(cfg)
}

/// Check whether the given filesystem path exists.
pub fn path_exists(p: &str) -> bool {
    Path::new(p).exists()
}
