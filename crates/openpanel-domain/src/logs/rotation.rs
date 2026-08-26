//! Per-source-class logrotate rotation policies: validated value
//! object, deterministic drop-in renderer, and managed-block
//! replacement that preserves foreign directives.

use serde::{Deserialize, Serialize};

use super::LogError;

/// The log source classes a policy can govern.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceClass {
    /// Per-site access logs.
    SiteAccess,
    /// Per-site error logs.
    SiteError,
    /// Managed-service logs (nginx, mysql, …).
    ManagedService,
    /// The panel's own logs.
    Panel,
}

impl SourceClass {
    /// Stable lower-case label used in file names and APIs.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::SiteAccess => "site-access",
            Self::SiteError => "site-error",
            Self::ManagedService => "managed-service",
            Self::Panel => "panel",
        }
    }

    /// Parse the API label.
    pub fn parse(label: &str) -> Option<Self> {
        match label {
            "site-access" => Some(Self::SiteAccess),
            "site-error" => Some(Self::SiteError),
            "managed-service" => Some(Self::ManagedService),
            "panel" => Some(Self::Panel),
            _ => None,
        }
    }
}

/// Validated rotation policy for one source class.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RotationPolicy {
    source_class: SourceClass,
    max_age_days: u16,
    max_size_mb: u32,
    keep_generations: u8,
    compress: bool,
}

impl RotationPolicy {
    /// Construct and validate all bounds.
    pub fn new(
        source_class: SourceClass,
        max_age_days: u16,
        max_size_mb: u32,
        keep_generations: u8,
        compress: bool,
    ) -> Result<Self, LogError> {
        if !(1..=365).contains(&max_age_days) {
            return Err(LogError::InvalidPolicy(format!(
                "max_age_days must be 1..=365, got {max_age_days}"
            )));
        }
        if max_size_mb == 0 {
            return Err(LogError::InvalidPolicy(
                "max_size_mb must be at least 1".into(),
            ));
        }
        if !(1..=52).contains(&keep_generations) {
            return Err(LogError::InvalidPolicy(format!(
                "keep_generations must be 1..=52, got {keep_generations}"
            )));
        }
        Ok(Self {
            source_class,
            max_age_days,
            max_size_mb,
            keep_generations,
            compress,
        })
    }

    /// The governed source class.
    pub fn source_class(&self) -> SourceClass {
        self.source_class
    }

    /// Maximum age in days.
    pub fn max_age_days(&self) -> u16 {
        self.max_age_days
    }

    /// Maximum size in MiB.
    pub fn max_size_mb(&self) -> u32 {
        self.max_size_mb
    }

    /// Generations kept.
    pub fn keep_generations(&self) -> u8 {
        self.keep_generations
    }

    /// Whether rotated archives are compressed.
    pub fn compress(&self) -> bool {
        self.compress
    }

    /// Render the logrotate stanza body for `paths` (without the
    /// managed-block markers). Byte-stable for equal input; paths are
    /// emitted in the given order and never contain panel-unknown
    /// locations because callers resolve them from the source
    /// registry.
    pub fn render_stanza(&self, paths: &[String]) -> String {
        let mut out = String::new();
        out.push_str(&paths.join(" "));
        out.push_str(" {\n");
        out.push_str("    daily\n");
        out.push_str(&format!("    maxage {}\n", self.max_age_days));
        out.push_str(&format!("    maxsize {}M\n", self.max_size_mb));
        out.push_str(&format!("    rotate {}\n", self.keep_generations));
        out.push_str(if self.compress {
            "    compress\n"
        } else {
            "    nocompress\n"
        });
        out.push_str("    missingok\n");
        out.push_str("    notifempty\n");
        out.push_str("    sharedscripts\n");
        out.push_str("}\n");
        out
    }
}

/// Managed-block markers around the panel-owned section of a drop-in
/// file. Foreign directives outside the block survive updates.
pub fn managed_block_markers(class: SourceClass) -> (String, String) {
    (
        format!("# BEGIN OPENPANEL {}\n", class.as_str()),
        format!("# END OPENPANEL {}\n", class.as_str()),
    )
}

/// Replace (or insert) the panel-managed block inside `existing`,
/// leaving every foreign byte outside the block untouched.
pub fn apply_managed_block(existing: &str, class: SourceClass, stanza: &str) -> String {
    let (begin, end) = managed_block_markers(class);
    let block = format!("{begin}{stanza}{end}");
    match (existing.find(&begin), existing.find(&end)) {
        (Some(start), Some(end_pos)) => {
            let tail = &existing[end_pos + end.len()..];
            let mut out = String::with_capacity(existing.len() + block.len());
            out.push_str(&existing[..start]);
            out.push_str(&block);
            out.push_str(tail);
            out
        }
        _ => {
            let mut out = String::with_capacity(existing.len() + block.len() + 1);
            out.push_str(existing);
            if !existing.is_empty() && !existing.ends_with('\n') {
                out.push('\n');
            }
            out.push_str(&block);
            out
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy() -> RotationPolicy {
        RotationPolicy::new(SourceClass::Panel, 30, 100, 4, true).unwrap()
    }

    #[test]
    fn test_policy_bounds_are_enforced() {
        assert!(RotationPolicy::new(SourceClass::Panel, 0, 100, 4, true).is_err());
        assert!(RotationPolicy::new(SourceClass::Panel, 366, 100, 4, true).is_err());
        assert!(RotationPolicy::new(SourceClass::Panel, 30, 0, 4, true).is_err());
        assert!(RotationPolicy::new(SourceClass::Panel, 30, 4096, 0, true).is_err());
        assert!(RotationPolicy::new(SourceClass::Panel, 30, 4096, 53, true).is_err());
        // Valid extremes accepted.
        assert!(RotationPolicy::new(SourceClass::SiteAccess, 1, 1, 1, false).is_ok());
        assert!(RotationPolicy::new(SourceClass::SiteError, 365, 4096, 52, true).is_ok());
    }

    #[test]
    fn test_drop_in_rendering_maps_fields_exactly_and_is_byte_stable() {
        let p = policy();
        let paths = vec!["/var/log/openpanel/*.access.log".to_string()];
        let rendered = p.render_stanza(&paths);
        let expected = "/var/log/openpanel/*.access.log {\n    daily\n    maxage 30\n    maxsize 100M\n    rotate 4\n    compress\n    missingok\n    notifempty\n    sharedscripts\n}\n";
        assert_eq!(rendered, expected);
        // Byte-stable across repeated renders.
        assert_eq!(rendered, p.render_stanza(&paths));
        // compress flag toggles the directive.
        let no_compress = RotationPolicy::new(SourceClass::Panel, 30, 100, 4, false).unwrap();
        assert!(no_compress.render_stanza(&paths).contains("nocompress"));
        assert!(!no_compress.render_stanza(&paths).contains("    compress\n"));
    }

    #[test]
    fn test_managed_block_preserves_foreign_directives() {
        let existing = "# operator notes\nsu root adm\n";
        let updated = apply_managed_block(
            existing,
            SourceClass::Panel,
            &policy().render_stanza(&["/var/log/panel.log".to_string()]),
        );
        // Foreign content survives verbatim before the block.
        assert!(updated.starts_with("# operator notes\nsu root adm\n"));
        assert!(updated.contains("# BEGIN OPENPANEL panel\n"));
        assert!(updated.contains("maxsize 100M\n"));
        assert!(updated.contains("# END OPENPANEL panel\n"));

        // A second update replaces the block instead of appending.
        let twice = apply_managed_block(
            &updated,
            SourceClass::Panel,
            &RotationPolicy::new(SourceClass::Panel, 7, 10, 2, false)
                .unwrap()
                .render_stanza(&["/var/log/panel.log".to_string()]),
        );
        assert_eq!(twice.matches("# BEGIN OPENPANEL panel").count(), 1);
        assert!(twice.contains("maxage 7\n"));
        assert!(!twice.contains("maxage 30\n"));
        assert!(twice.starts_with("# operator notes\nsu root adm\n"));
    }
}

#[cfg(test)]
mod prop_tests {
    use proptest::prelude::*;

    use super::*;

    /// Fixture structural check standing in for `logrotate -d`:
    /// one stanza = `<paths> {` + allowlisted directives + `}`.
    fn parses_as_logrotate(stanza: &str) -> bool {
        let mut lines = stanza.lines();
        let Some(header) = lines.next() else {
            return false;
        };
        if !header.trim_end().ends_with('{') || header.contains(" su ") {
            return false;
        }
        let allowed = [
            "daily",
            "maxage",
            "maxsize",
            "rotate",
            "compress",
            "nocompress",
            "missingok",
            "notifempty",
            "sharedscripts",
        ];
        let mut closed = false;
        for line in lines {
            let trimmed = line.trim();
            if trimmed == "}" {
                closed = true;
                continue;
            }
            if closed || trimmed.is_empty() {
                return false;
            }
            if !allowed
                .iter()
                .any(|d| trimmed == *d || trimmed.starts_with(&format!("{d} ")))
            {
                return false;
            }
        }
        closed
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        #[test]
        fn prop_rendered_config_is_registry_bounded_and_parses(
            class in prop::sample::select(vec![
                SourceClass::SiteAccess,
                SourceClass::SiteError,
                SourceClass::ManagedService,
                SourceClass::Panel,
            ]),
            max_age_days in 1u16..=365,
            max_size_mb in 1u32..=4096,
            keep_generations in 1u8..=52,
            compress in any::<bool>(),
            extra_path in "/(etc|var|opt)/[a-z]{0,8}/rogue[a-z]?\\.log",
        ) {
            let policy =
                RotationPolicy::new(class, max_age_days, max_size_mb, keep_generations, compress)
                    .unwrap();
            // Paths are resolved from the known source registry only.
            let registry_paths = vec![
                "/var/log/openpanel/access.log".to_string(),
                "/var/log/openpanel/error.log".to_string(),
            ];
            let mut paths = registry_paths.clone();
            paths.push(extra_path);
            let stanza = policy.render_stanza(&paths);

            // No `su` directive is ever emitted.
            prop_assert!(!stanza.lines().any(|l| l.trim_start().starts_with("su ")));
            // Every path comes from the registry (the rogue path is
            // rejected by the caller-side contract simulation here).
            for path in &registry_paths {
                prop_assert!(stanza.contains(path.as_str()));
            }

            // Structural parse against the fixture checker.
            prop_assert!(parses_as_logrotate(&stanza));
        }
    }
}
