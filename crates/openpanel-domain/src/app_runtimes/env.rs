//! Per-runtime environment variables and secrets: validated key/value
//! sets and the supervisor-unit env branch. Secrets never appear in
//! the unit text — they ride an `EnvironmentFile=` written 0600 by
//! the application layer inside the chroot.

use serde::{Deserialize, Serialize};

/// Keys reserved for the host system; runtimes may not set them.
pub const RESERVED_KEYS: [&str; 3] = ["PATH", "HOME", "USER"];

/// Maximum number of variables per set.
pub const MAX_VARS: usize = 128;
/// Maximum serialized size of a set (64 KiB).
pub const MAX_SET_BYTES: usize = 64 * 1024;
/// Maximum plaintext length of one non-secret value (8 KiB).
pub const MAX_PLAIN_VALUE_BYTES: usize = 8 * 1024;

/// Validation failures for environment configuration.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EnvError {
    /// The key is empty, malformed, or longer than 128 chars.
    #[error("invalid env key: {0}")]
    InvalidKey(String),
    /// The key collides with a host-reserved variable.
    #[error("reserved env key: {0}")]
    ReservedKey(String),
    /// The same key appears twice.
    #[error("duplicate env key: {0}")]
    DuplicateKey(String),
    /// A value exceeds its size cap.
    #[error("env value too large: {0}")]
    ValueTooLarge(String),
    /// The set exceeds the variable-count or byte-size cap.
    #[error("env set too large")]
    SetTooLarge,
}

/// A validated environment variable key: `[A-Z_][A-Z0-9_]*`, at most
/// 128 chars, never host-reserved.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EnvKey(String);

impl EnvKey {
    /// Construct and validate.
    pub fn new(key: impl Into<String>) -> Result<Self, EnvError> {
        let key = key.into();
        if key.is_empty() || key.len() > 128 {
            return Err(EnvError::InvalidKey(key));
        }
        let first = key.as_bytes()[0];
        if !(first.is_ascii_uppercase() || first == b'_') {
            return Err(EnvError::InvalidKey(key.clone()));
        }
        if !key
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
        {
            return Err(EnvError::InvalidKey(key.clone()));
        }
        if RESERVED_KEYS.contains(&key.as_str()) {
            return Err(EnvError::ReservedKey(key));
        }
        Ok(Self(key))
    }

    /// The key string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// One variable: key, value, and whether the value is a secret.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnvVar {
    /// Variable key.
    pub key: EnvKey,
    /// Plaintext value (secrets are encrypted only at rest by the
    /// application layer).
    pub value: String,
    /// Whether the value must ride the 0600 env file.
    pub secret: bool,
}

impl EnvVar {
    /// Construct and validate the key and plain-value cap.
    pub fn new(
        key: impl Into<String>,
        value: impl Into<String>,
        secret: bool,
    ) -> Result<Self, EnvError> {
        let key = EnvKey::new(key)?;
        let value = value.into();
        if !secret && value.len() > MAX_PLAIN_VALUE_BYTES {
            return Err(EnvError::ValueTooLarge(key.as_str().to_string()));
        }
        Ok(Self { key, value, secret })
    }
}

/// An ordered, unique set of variables with size caps.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EnvSet {
    vars: Vec<EnvVar>,
}

impl EnvSet {
    /// Construct from variables. Duplicate keys are rejected;
    /// insertion order is preserved.
    pub fn new(vars: Vec<EnvVar>) -> Result<Self, EnvError> {
        if vars.len() > MAX_VARS {
            return Err(EnvError::SetTooLarge);
        }
        let mut seen = std::collections::HashSet::new();
        let mut total = 0usize;
        for var in &vars {
            if !seen.insert(var.key.as_str().to_string()) {
                return Err(EnvError::DuplicateKey(var.key.as_str().to_string()));
            }
            total += var.key.as_str().len() + var.value.len() + 2;
        }
        if total > MAX_SET_BYTES {
            return Err(EnvError::SetTooLarge);
        }
        Ok(Self { vars })
    }

    /// Build an empty set.
    pub fn empty() -> Self {
        Self { vars: Vec::new() }
    }

    /// Variables in insertion order.
    pub fn vars(&self) -> &[EnvVar] {
        &self.vars
    }

    /// Whether any variable is marked secret.
    pub fn has_secrets(&self) -> bool {
        self.vars.iter().any(|v| v.secret)
    }

    /// Non-secret `KEY=value` lines for the unit's `Environment=`.
    pub fn render_environment_lines(&self) -> String {
        self.vars
            .iter()
            .filter(|v| !v.secret)
            .map(|v| format!("Environment={}={}\n", v.key.as_str(), v.value))
            .collect()
    }

    /// `KEY=value` lines for the 0600 env file (all variables).
    pub fn render_env_file(&self) -> String {
        self.vars
            .iter()
            .map(|v| format!("{}={}\n", v.key.as_str(), v.value))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_env_key_validation() {
        assert!(EnvKey::new("DATABASE_URL").is_ok());
        assert!(EnvKey::new("_X1").is_ok());
        assert!(EnvKey::new("1BAD").is_err());
        assert!(EnvKey::new("A-B").is_err());
        assert!(EnvKey::new("").is_err());
        assert!(EnvKey::new("A".repeat(129)).is_err());
        assert_eq!(
            EnvKey::new("PATH").unwrap_err(),
            EnvError::ReservedKey("PATH".into())
        );
        assert_eq!(
            EnvKey::new("HOME").unwrap_err(),
            EnvError::ReservedKey("HOME".into())
        );
        assert_eq!(
            EnvKey::new("USER").unwrap_err(),
            EnvError::ReservedKey("USER".into())
        );
    }

    #[test]
    fn test_env_set_uniqueness_and_ordering() {
        let ok = EnvSet::new(vec![
            EnvVar::new("A", "1", false).unwrap(),
            EnvVar::new("B", "2", false).unwrap(),
        ])
        .unwrap();
        assert_eq!(
            ok.vars().iter().map(|v| v.key.as_str()).collect::<Vec<_>>(),
            vec!["A", "B"]
        );
        // Duplicates collapse into an error on construction.
        let dup = EnvSet::new(vec![
            EnvVar::new("A", "1", false).unwrap(),
            EnvVar::new("A", "2", false).unwrap(),
        ]);
        assert_eq!(dup.unwrap_err(), EnvError::DuplicateKey("A".to_string()));
    }

    #[test]
    fn test_size_caps() {
        // Single plain value over 8 KiB rejected.
        assert!(EnvVar::new("BIG", "x".repeat(8 * 1024 + 1), false).is_err());
        // Secret values are exempt from the plain cap.
        assert!(EnvVar::new("BIG_SECRET", "x".repeat(8 * 1024 + 1), true).is_ok());
        // More than 128 vars rejected.
        let too_many: Vec<EnvVar> = (0..129)
            .map(|i| EnvVar::new(format!("K{i}"), "v", false).unwrap())
            .collect();
        assert_eq!(EnvSet::new(too_many).unwrap_err(), EnvError::SetTooLarge);
        // Serialized size over 64 KiB rejected.
        let big: Vec<EnvVar> = (0..9)
            .map(|i| EnvVar::new(format!("K{i}"), "x".repeat(7_500), false).unwrap())
            .collect();
        assert_eq!(EnvSet::new(big).unwrap_err(), EnvError::SetTooLarge);
    }
}

#[cfg(test)]
mod golden_and_prop_tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use chrono::Utc;
    use proptest::prelude::*;
    use uuid::Uuid;

    use super::*;
    use crate::app_runtimes::{
        RuntimeKind, RuntimeStatus, SiteRuntime, render_supervisor_unit,
        render_supervisor_unit_with_env,
    };

    fn fixture_runtime() -> SiteRuntime {
        SiteRuntime {
            id: Uuid::from_u128(7),
            site_id: Uuid::from_u128(8),
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
    fn golden_env_branch_matches_pre_change_bytes_and_secret_switch() {
        let runtime = fixture_runtime();
        let user = "site-user";
        let home = "/srv/site";

        // Golden: the pre-change unit bytes for this fixture.
        let expected = "[Unit]\nDescription=OpenPanel site runtime 00000000-0000-0000-0000-000000000008 (node)\nAfter=network.target\n\n[Service]\nType=simple\nUser=site-user\nWorkingDirectory=/srv/site/app\nExecStart=/usr/bin/env -S /bin/sh -c 'exec app'\nRestart=on-failure\nRestartSec=5\nEnvironment=APP_PORT=3000\n\n[Install]\nWantedBy=multi-user.target\n";
        assert_eq!(render_supervisor_unit(&runtime, user, home), expected);
        assert_eq!(
            render_supervisor_unit_with_env(&runtime, user, home, None),
            expected
        );
        assert_eq!(
            render_supervisor_unit_with_env(&runtime, user, home, Some(&EnvSet::empty())),
            expected
        );

        // One non-secret var adds exactly one Environment= line.
        let plain = EnvSet::new(vec![EnvVar::new("LOG_LEVEL", "debug", false).unwrap()]).unwrap();
        let with_plain = render_supervisor_unit_with_env(&runtime, user, home, Some(&plain));
        assert!(with_plain.contains("Environment=LOG_LEVEL=debug\n"));
        assert_eq!(with_plain.matches("Environment=").count(), 2); // APP_PORT + LOG_LEVEL

        // Any secret switches to EnvironmentFile and leaks no value.
        let secret = EnvSet::new(vec![
            EnvVar::new("DATABASE_URL", "postgres://s3cret", true).unwrap(),
            EnvVar::new("LOG_LEVEL", "debug", false).unwrap(),
        ])
        .unwrap();
        let with_secret = render_supervisor_unit_with_env(&runtime, user, home, Some(&secret));
        assert!(with_secret.contains("EnvironmentFile=-/srv/site/app/.openpanel-env"));
        assert!(!with_secret.contains("s3cret"));
        assert!(!with_secret.contains("Environment=DATABASE_URL"));
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        #[test]
        fn prop_secrets_never_in_unit_and_keys_unique(
            keys in proptest::collection::vec("[A-Z][A-Z0-9_]{0,12}", 1..8),
            values in proptest::collection::vec("[a-zA-Z0-9]{8,32}", 1..8),
            secret_flags in proptest::collection::vec(any::<bool>(), 1..8),
        ) {
            let mut vars = Vec::new();
            for ((key, value), secret) in keys.iter().zip(values.iter()).zip(secret_flags.iter()) {
                if let Ok(var) = EnvVar::new(key.clone(), value.clone(), *secret) {
                    vars.push(var);
                }
            }
            let set = match EnvSet::new(vars) {
                Ok(set) => set,
                Err(_) => return Ok(()), // duplicates collapsed by strategy overlap
            };
            let runtime = fixture_runtime();
            let unit = render_supervisor_unit_with_env(
                &runtime,
                "u",
                "/srv",
                Some(&set),
            );
            // Secret values never appear in the unit text.
            for var in set.vars().iter().filter(|v| v.secret) {
                prop_assert!(!unit.contains(var.value.as_str()));
            }
            // Every key appears at most once in the rendered env lines.
            for var in set.vars() {
                let marker = format!("{}=", var.key.as_str());
                prop_assert!(unit.matches(&marker).count() <= 1);
            }
            ()
        }
    }
}
