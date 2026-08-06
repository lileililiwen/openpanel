//! Config layering: built-in defaults → `/etc/openpanel/openpanel.toml` →
//! `$OPENPANEL_CONFIG` → `OPENPANEL__*` env vars → CLI overrides.
//!
//! Final config is validated against a built-in JSON Schema. On failure,
//! startup aborts with exit code 78 (`EX_CONFIG`).

use std::path::Path;
use std::sync::Arc;

use figment::providers::{Env, Format, Toml};
use figment::Figment;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{ConfigError, CoreResult};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub bind: String,
    pub port: u16,
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
pub struct DatabaseConfig {
    pub driver: String,
    pub url: String,
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
pub struct LogConfig {
    pub level: String,
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
pub struct Config {
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default)]
    pub database: DatabaseConfig,
    #[serde(default)]
    pub log: LogConfig,
    #[serde(default)]
    pub modules: serde_json::Map<String, Value>,
}

impl Config {
    /// Load config from disk + env. CLI overrides are applied by the caller
    /// through `with_overrides`.
    pub fn load() -> CoreResult<Arc<Self>> {
        let defaults = toml::to_string(&Config::default())
            .map_err(|e| ConfigError::Load(e.to_string()))?;
        let mut figment = Figment::new().merge(Toml::string(&defaults));

        if path_exists("/etc/openpanel/openpanel.toml") {
            figment = figment.merge(Toml::file("/etc/openpanel/openpanel.toml").nested());
        }
        if let Ok(user_path) = std::env::var("OPENPANEL_CONFIG") {
            if path_exists(&user_path) {
                figment = figment.merge(Toml::file(user_path).nested());
            }
        }
        figment = figment.merge(Env::prefixed("OPENPANEL__").split("__"));

        let cfg: Config = figment
            .extract()
            .map_err(|e| ConfigError::Load(e.to_string()))?;

        cfg.validate()?;
        Ok(Arc::new(cfg))
    }

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
                "modules": {"type": "object"},
            },
            "required": ["server", "database"],
        });

        let validator = jsonschema::validator_for(&schema)
            .map_err(|e| ConfigError::Schema(e.to_string()))?;
        let value = serde_json::to_value(self)
            .map_err(|e| ConfigError::Load(e.to_string()))?;
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

    pub fn server(&self) -> &ServerConfig {
        &self.server
    }
    pub fn database(&self) -> &DatabaseConfig {
        &self.database
    }
    pub fn log(&self) -> &LogConfig {
        &self.log
    }
    pub fn module_config(&self, name: &str) -> Option<&Value> {
        self.modules.get(name)
    }
}

/// Apply a CLI override (`path = value`) into the config, returning the
/// updated config for validation.
pub fn with_overrides(mut cfg: Config, path: &str, value: Value) -> CoreResult<Config> {
    let segments: Vec<&str> = path.split('.').collect();
    let mut current = serde_json::to_value(&cfg)
        .map_err(|e| ConfigError::Load(e.to_string()))?;
    let mut cursor: &mut Value = &mut current;
    for seg in &segments {
        if !cursor.is_object() {
            return Err(ConfigError::Load(format!("path `{path}` is not an object")).into());
        }
        let obj = cursor.as_object_mut().unwrap();
        if !obj.contains_key(*seg) {
            obj.insert((*seg).to_string(), Value::Null);
        }
        cursor = obj.get_mut(*seg).unwrap();
    }
    *cursor = value;
    let updated: Config = serde_json::from_value(current)
        .map_err(|e| ConfigError::Load(e.to_string()))?;
    cfg = updated;
    Ok(cfg)
}

pub fn path_exists(p: &str) -> bool {
    Path::new(p).exists()
}