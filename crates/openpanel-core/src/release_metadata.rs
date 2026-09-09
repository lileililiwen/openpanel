//! Release metadata embedded in the binary at compile time.
//!
//! `BinaryMetadata` describes what was built and the highest database
//! schema version this binary knows how to migrate up to. The schema
//! ceiling is consumed by `openpanel_app::release_preflight` to refuse
//! a startup that would otherwise mutate a store it does not
//! understand (see `release-deployment-governance` spec, "Safe Upgrade
//! and Rollback").
//!
//! The fields are sourced, in this strict precedence order:
//!
//! 1. Compile-time `option_env!("OPENPANEL_MAX_SCHEMA_VERSION")` (set
//!    by the release workflow via `OPENPANEL_MAX_SCHEMA_VERSION=<v>`).
//! 2. The runtime `OPENPANEL__BUILD__*` env vars, which override the
//!    compile-time values (used by integration tests).
//! 3. The compile-time defaults:
//!    - `version = CARGO_PKG_VERSION`
//!    - `commit = "unknown"`
//!    - `target = "unknown"`
//!    - `max_schema_version = "0"`
//!    - `build_timestamp = "unknown"`

use std::env;

const ENV_VERSION: &str = "OPENPANEL__BUILD__VERSION";
const ENV_COMMIT: &str = "OPENPANEL__BUILD__COMMIT";
const ENV_TARGET: &str = "OPENPANEL__BUILD__TARGET";
const ENV_TIMESTAMP: &str = "OPENPANEL__BUILD__TIMESTAMP";
const ENV_MAX_SCHEMA: &str = "OPENPANEL__BUILD__MAX_SCHEMA_VERSION";

const DEFAULT_COMMIT: &str = "unknown";
const DEFAULT_TARGET: &str = "unknown";
const DEFAULT_TIMESTAMP: &str = "unknown";

const MAX_SUPPORTED_SCHEMA_VERSION: &str = match option_env!("OPENPANEL_MAX_SCHEMA_VERSION") {
    Some(v) => v,
    None => "0",
};

/// Compile-time + runtime release metadata. Cheap to copy, safe to log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BinaryMetadata {
    /// `Cargo.toml` version (e.g. `0.1.0`).
    pub version: String,
    /// Git commit the binary was built from (`unknown` when built
    /// outside a git checkout).
    pub commit: String,
    /// Cargo target triple the binary was built for
    /// (e.g. `x86_64-unknown-linux-gnu`).
    pub target: String,
    /// ISO-8601 build timestamp (or `unknown`).
    pub build_timestamp: String,
    /// Highest database schema version this binary can apply. Any
    /// `_migrations` row above this value blocks startup.
    pub max_schema_version: String,
}

impl BinaryMetadata {
    /// Resolve the metadata from compile-time defaults, then overlay
    /// the runtime env vars when present.
    pub fn load() -> Self {
        let compile_version = env!("CARGO_PKG_VERSION").to_string();
        Self::from_env_map(
            [
                (ENV_VERSION, compile_version),
                (ENV_COMMIT, DEFAULT_COMMIT.to_string()),
                (ENV_TARGET, DEFAULT_TARGET.to_string()),
                (ENV_TIMESTAMP, DEFAULT_TIMESTAMP.to_string()),
                (ENV_MAX_SCHEMA, MAX_SUPPORTED_SCHEMA_VERSION.to_string()),
            ],
            env::vars(),
        )
    }

    /// Build metadata from a pre-collected defaults map and an
    /// environment map. Tests pass a deterministic map; production
    /// uses `load`, which calls this with `env::vars()`.
    pub fn from_env_map<I, K, V, J, K2, V2>(defaults: I, env_vars: J) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        J: IntoIterator<Item = (K2, V2)>,
        K: AsRef<str>,
        V: AsRef<str>,
        K2: AsRef<str>,
        V2: AsRef<str>,
    {
        let mut base: std::collections::HashMap<String, String> = defaults
            .into_iter()
            .map(|(k, v)| (k.as_ref().to_string(), v.as_ref().to_string()))
            .collect();
        for (k, v) in env_vars {
            let key = k.as_ref();
            if [
                ENV_VERSION,
                ENV_COMMIT,
                ENV_TARGET,
                ENV_TIMESTAMP,
                ENV_MAX_SCHEMA,
            ]
            .contains(&key)
            {
                base.insert(key.to_string(), v.as_ref().to_string());
            }
        }
        Self {
            version: base.remove(ENV_VERSION).unwrap_or_default(),
            commit: base.remove(ENV_COMMIT).unwrap_or_default(),
            target: base.remove(ENV_TARGET).unwrap_or_default(),
            build_timestamp: base.remove(ENV_TIMESTAMP).unwrap_or_default(),
            max_schema_version: base.remove(ENV_MAX_SCHEMA).unwrap_or_default(),
        }
    }

    /// Parse `max_schema_version` as `u32`. Returns `0` for
    /// unparseable values — never panics, never returns `Err`. The
    /// preflight treats `0` as "no schema ceiling", which is the
    /// safe default for a dev build.
    pub fn max_schema_version_u32(&self) -> u32 {
        self.max_schema_version.trim().parse::<u32>().unwrap_or(0)
    }

    /// True when both metadata blocks parse to the same
    /// `max_schema_version`. Used by the upgrade smoke test to assert
    /// the in-memory metadata is the one the installer just deployed.
    pub fn schema_ceiling_matches(&self, other: &BinaryMetadata) -> bool {
        self.max_schema_version_u32() == other.max_schema_version_u32()
    }
}

/// Convenience accessor used by the preflight and the integration
/// smoke tests. Equivalent to `BinaryMetadata::load()`.
pub fn current_metadata() -> BinaryMetadata {
    BinaryMetadata::load()
}

/// Highest schema version compiled into this binary. Equal to
/// `BinaryMetadata::load().max_schema_version_u32()`.
pub fn compiled_max_schema_version() -> u32 {
    MAX_SUPPORTED_SCHEMA_VERSION.parse::<u32>().unwrap_or(0)
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;

    fn default_map() -> Vec<(&'static str, String)> {
        vec![
            (ENV_VERSION, env!("CARGO_PKG_VERSION").to_string()),
            (ENV_COMMIT, DEFAULT_COMMIT.to_string()),
            (ENV_TARGET, DEFAULT_TARGET.to_string()),
            (ENV_TIMESTAMP, DEFAULT_TIMESTAMP.to_string()),
            (ENV_MAX_SCHEMA, MAX_SUPPORTED_SCHEMA_VERSION.to_string()),
        ]
    }

    #[test]
    fn current_with_no_overrides_returns_compile_time_defaults() {
        let saved: Vec<(String, String)> = env::vars().collect();
        let m = BinaryMetadata::from_env_map(default_map(), std::iter::empty::<(&str, &str)>());
        assert!(!m.version.is_empty(), "version must be non-empty");
        assert_eq!(m.commit, DEFAULT_COMMIT);
        assert_eq!(m.target, DEFAULT_TARGET);
        assert_eq!(m.build_timestamp, DEFAULT_TIMESTAMP);
        let _ = saved;
    }

    #[test]
    fn env_map_overrides_defaults() {
        let m = BinaryMetadata::from_env_map(
            default_map(),
            [
                (ENV_COMMIT, "deadbeef"),
                (ENV_TARGET, "aarch64-unknown-linux-musl"),
                (ENV_TIMESTAMP, "2026-09-09T12:00:00Z"),
                (ENV_MAX_SCHEMA, "42"),
            ],
        );
        assert_eq!(m.commit, "deadbeef");
        assert_eq!(m.target, "aarch64-unknown-linux-musl");
        assert_eq!(m.build_timestamp, "2026-09-09T12:00:00Z");
        assert_eq!(m.max_schema_version, "42");
    }

    #[test]
    fn unrelated_env_vars_are_ignored() {
        let m = BinaryMetadata::from_env_map(
            default_map(),
            [("UNRELATED_VAR", "noise"), ("PATH", "/usr/bin")],
        );
        assert_eq!(m.commit, DEFAULT_COMMIT);
        assert_eq!(m.target, DEFAULT_TARGET);
    }

    #[test]
    fn max_schema_version_parses_when_numeric() {
        let mut m = BinaryMetadata::load();
        m.max_schema_version = "7".to_string();
        assert_eq!(m.max_schema_version_u32(), 7);
    }

    #[test]
    fn max_schema_version_falls_back_to_zero_for_unparseable_values() {
        let mut m = BinaryMetadata::load();
        m.max_schema_version = "not-a-number".to_string();
        assert_eq!(m.max_schema_version_u32(), 0);
        m.max_schema_version = "".to_string();
        assert_eq!(m.max_schema_version_u32(), 0);
        m.max_schema_version = "  ".to_string();
        assert_eq!(m.max_schema_version_u32(), 0);
    }

    #[test]
    fn schema_ceiling_matches_is_strictly_numeric() {
        let mut a = BinaryMetadata::load();
        let mut b = BinaryMetadata::load();
        a.max_schema_version = "5".to_string();
        b.max_schema_version = "5".to_string();
        assert!(a.schema_ceiling_matches(&b));
        b.max_schema_version = "6".to_string();
        assert!(!a.schema_ceiling_matches(&b));
    }

    #[test]
    fn compiled_max_schema_version_is_a_non_negative_integer() {
        let n = compiled_max_schema_version();
        assert!(n < u32::MAX, "compiled ceiling must parse");
    }
}
