//! Per-file line-count thresholds and the file-length lint pipeline.
//!
//! The OpenPanel quality spec defines a soft limit (warning) and a
//! hard limit (build failure) for every `.rs` file in the workspace.
//! The thresholds are resolved in this strict precedence order:
//!
//! 1. Environment variables `OPENPANEL_FILE_LENGTH_SOFT_LIMIT` and
//!    `OPENPANEL_FILE_LENGTH_HARD_LIMIT` (the highest priority — used
//!    by CI, contributors, and per-branch overrides).
//! 2. The `lint-extra.toml` file at the repo root, parsed when
//!    `load_from_file` is called.
//! 3. The compile-time defaults (`DEFAULT_SOFT_LIMIT = 850`,
//!    `DEFAULT_HARD_LIMIT = 1000`).
//!
//! `print-thresholds` is a small binary in this crate that prints
//! the effective values so `scripts/check-file-length.sh` and the
//! Rust app share one source of truth.

use std::path::Path;

use serde::Deserialize;

use crate::error::CoreError;

/// Default soft limit (lines) — emits a warning, does not fail the build.
pub const DEFAULT_SOFT_LIMIT: u32 = 850;
/// Default hard limit (lines) — fails the build.
pub const DEFAULT_HARD_LIMIT: u32 = 1000;
/// Default exclude globs — paths the lint should never scan.
pub const DEFAULT_EXCLUDES: &[&str] = &[
    "**/target/**",
    "**/node_modules/**",
    "**/.git/**",
    "**/dist/**",
    "**/proptest-regressions/**",
    "**/tests/fixtures/**",
    "crates/openpanel-test-support/src/snapshot/**",
];

/// The on-disk shape of `lint-extra.toml`.
#[derive(Debug, Clone, Deserialize)]
pub struct LintExtraFile {
    /// Soft warning threshold.
    pub soft_limit: Option<u32>,
    /// Hard failure threshold.
    pub hard_limit: Option<u32>,
    /// Glob list of paths to exclude (in addition to the defaults).
    #[serde(default)]
    pub exclude: Vec<String>,
}

/// Resolved, effective file-length thresholds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileLengthThresholds {
    /// Soft warning threshold.
    pub soft_limit: u32,
    /// Hard failure threshold.
    pub hard_limit: u32,
    /// Effective exclude list (defaults merged with the file's list).
    pub exclude: Vec<String>,
}

impl FileLengthThresholds {
    /// Compile-time defaults, with no file-derived overrides and no env vars.
    pub fn default_values() -> Self {
        Self {
            soft_limit: DEFAULT_SOFT_LIMIT,
            hard_limit: DEFAULT_HARD_LIMIT,
            exclude: DEFAULT_EXCLUDES.iter().map(|s| (*s).to_string()).collect(),
        }
    }

    /// Build the thresholds from a `lint-extra.toml` shape, then layer
    /// the env vars on top.
    pub fn from_file(file: &LintExtraFile) -> Self {
        let defaults = Self::default_values();
        Self {
            soft_limit: file.soft_limit.unwrap_or(defaults.soft_limit),
            hard_limit: file.hard_limit.unwrap_or(defaults.hard_limit),
            exclude: merge_excludes(&defaults.exclude, &file.exclude),
        }
    }

    /// Build the thresholds from a TOML file on disk, then layer the
    /// env vars on top.
    pub fn from_path(path: impl AsRef<Path>) -> Result<Self, CoreError> {
        let raw = std::fs::read_to_string(path.as_ref())
            .map_err(|err| CoreError::Config(crate::error::ConfigError::Load(err.to_string())))?;
        let parsed: LintExtraFile = toml::from_str(&raw)
            .map_err(|err| CoreError::Config(crate::error::ConfigError::Load(err.to_string())))?;
        Ok(Self::from_file(&parsed))
    }

    /// Build the thresholds from environment variables, layering on
    /// top of the provided base (defaults or file-derived).
    pub fn from_env(base: Self) -> Self {
        let collected: Vec<(String, String)> = std::env::vars().collect();
        Self::from_env_map(base, collected)
    }

    /// Build the thresholds from a pre-collected environment-variable
    /// map. Tests pass a deterministic map; production uses
    /// `from_env`, which calls this with `std::env::vars()`.
    pub fn from_env_map<I, K, V>(base: Self, vars: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: AsRef<str>,
        V: AsRef<str>,
    {
        let mut soft = base.soft_limit;
        let mut hard = base.hard_limit;
        for (k, v) in vars {
            let key = k.as_ref();
            let value = v.as_ref();
            if key == "OPENPANEL_FILE_LENGTH_SOFT_LIMIT"
                && let Some(parsed) = parse_u32(value)
            {
                soft = parsed;
            } else if key == "OPENPANEL_FILE_LENGTH_HARD_LIMIT"
                && let Some(parsed) = parse_u32(value)
            {
                hard = parsed;
            }
        }
        Self {
            soft_limit: soft,
            hard_limit: hard,
            exclude: base.exclude,
        }
    }

    /// Convenience: defaults → env. Used by `print-thresholds` when
    /// no `lint-extra.toml` is present.
    pub fn resolved() -> Self {
        Self::from_env(Self::default_values())
    }

    /// Load from file at the given path (if it exists), then layer env.
    /// Missing file → fall back to defaults → env.
    pub fn load(path: impl AsRef<Path>) -> Self {
        let base = Self::from_path(&path).unwrap_or_else(|_| Self::default_values());
        Self::from_env(base)
    }

    /// True if `lines` is at or above the hard limit.
    pub fn exceeds_hard(&self, lines: u32) -> bool {
        lines >= self.hard_limit
    }

    /// True if `lines` is at or above the soft limit but below the hard.
    pub fn exceeds_soft(&self, lines: u32) -> bool {
        lines >= self.soft_limit && lines < self.hard_limit
    }

    /// True if `path` matches one of the exclude globs.
    pub fn is_excluded(&self, path: &str) -> bool {
        let path = Path::new(path);
        for pattern in &self.exclude {
            if pattern_matches(pattern, path) {
                return true;
            }
        }
        false
    }
}

fn parse_u32(input: &str) -> Option<u32> {
    input.trim().parse::<u32>().ok()
}

fn merge_excludes(defaults: &[String], file: &[String]) -> Vec<String> {
    let mut out: Vec<String> = defaults.to_vec();
    for entry in file {
        if !out.iter().any(|existing| existing == entry) {
            out.push(entry.clone());
        }
    }
    out
}

fn pattern_matches(pattern: &str, path: &Path) -> bool {
    glob::Pattern::new(pattern)
        .map(|p| p.matches_path(path))
        .unwrap_or(false)
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn default_values_match_the_spec() {
        let t = FileLengthThresholds::default_values();
        assert_eq!(t.soft_limit, 850);
        assert_eq!(t.hard_limit, 1000);
        assert!(t.exclude.iter().any(|s| s == "**/target/**"));
    }

    #[test]
    fn env_overrides_default_soft() {
        let base = FileLengthThresholds::default_values();
        let t =
            FileLengthThresholds::from_env_map(base, [("OPENPANEL_FILE_LENGTH_SOFT_LIMIT", "100")]);
        assert_eq!(t.soft_limit, 100);
        assert_eq!(t.hard_limit, DEFAULT_HARD_LIMIT);
    }

    #[test]
    fn env_overrides_default_hard() {
        let base = FileLengthThresholds::default_values();
        let t = FileLengthThresholds::from_env_map(
            base,
            [("OPENPANEL_FILE_LENGTH_HARD_LIMIT", "2000")],
        );
        assert_eq!(t.hard_limit, 2000);
    }

    #[test]
    fn invalid_env_falls_back_to_default() {
        let base = FileLengthThresholds::default_values();
        let t = FileLengthThresholds::from_env_map(
            base,
            [("OPENPANEL_FILE_LENGTH_SOFT_LIMIT", "not-a-number")],
        );
        assert_eq!(t.soft_limit, DEFAULT_SOFT_LIMIT);
    }

    #[test]
    fn from_file_overrides_default() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("lint-extra.toml");
        fs::write(
            &path,
            "soft_limit = 100\nhard_limit = 200\nexclude = [\"**/foo/**\"]\n",
        )
        .expect("write");
        let t = FileLengthThresholds::load(&path);
        assert_eq!(t.soft_limit, 100);
        assert_eq!(t.hard_limit, 200);
        assert!(t.exclude.iter().any(|s| s == "**/foo/**"));
        assert!(t.exclude.iter().any(|s| s == "**/target/**"));
    }

    #[test]
    fn from_file_missing_falls_back_to_defaults() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("does-not-exist.toml");
        let t = FileLengthThresholds::load(&path);
        assert_eq!(t.soft_limit, DEFAULT_SOFT_LIMIT);
        assert_eq!(t.hard_limit, DEFAULT_HARD_LIMIT);
    }

    #[test]
    fn exceeds_hard_and_soft() {
        let t = FileLengthThresholds::default_values();
        assert!(!t.exceeds_hard(999));
        assert!(t.exceeds_hard(1000));
        assert!(!t.exceeds_soft(849));
        assert!(t.exceeds_soft(900));
        assert!(!t.exceeds_soft(1000));
    }

    #[test]
    fn env_beats_file() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("lint-extra.toml");
        fs::write(&path, "soft_limit = 100\nhard_limit = 200\n").expect("write");
        let from_file = FileLengthThresholds::load(&path);
        let t = FileLengthThresholds::from_env_map(
            from_file,
            [("OPENPANEL_FILE_LENGTH_SOFT_LIMIT", "300")],
        );
        assert_eq!(t.soft_limit, 300);
        assert_eq!(t.hard_limit, 200);
    }
}
