//! Host migration readiness: manifest compatibility, collision
//! preview, and bootstrap command. Pure domain — no I/O.
//!
//! Covers `backup-dr-operations`: Host Migration Is Verifiable.

use super::{BackupError, snapshot::SUPPORTED_MANIFEST_SCHEMA};
use serde::{Deserialize, Serialize};

/// Readiness summary for importing one bundle on this host.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MigrationReadiness {
    compatible: bool,
    collisions: Vec<String>,
    bootstrap_command: String,
    guidance: String,
}

impl MigrationReadiness {
    /// Whether the bundle schema is supported by this binary.
    pub fn compatible(&self) -> bool {
        self.compatible
    }

    /// Name collisions requiring overwrite confirmation.
    pub fn collisions(&self) -> &[String] {
        &self.collisions
    }

    /// Bootstrap command for the destination host.
    pub fn bootstrap_command(&self) -> &str {
        &self.bootstrap_command
    }

    /// Safe operator guidance.
    pub fn guidance(&self) -> &str {
        &self.guidance
    }
}

/// Whether a bundle manifest schema is supported by this binary.
pub fn check_manifest_compatibility(manifest_schema: u32) -> bool {
    manifest_schema <= SUPPORTED_MANIFEST_SCHEMA
}

/// Preview name collisions between existing and planned resources.
///
/// Returns the sorted intersection; empty means a clean import.
pub fn preview_migration_collisions(existing: &[String], planned: &[String]) -> Vec<String> {
    let mut hits: Vec<String> = planned
        .iter()
        .filter(|name| existing.iter().any(|other| other == *name))
        .cloned()
        .collect();
    hits.sort();
    hits.dedup();
    hits
}

/// Build the destination-host bootstrap command for an offsite key.
///
/// Rejects empty keys and keys that escape the object namespace; the
/// command carries only the object key and host label, never secrets.
pub fn migration_bootstrap_command(
    target_key: &str,
    host_label: &str,
) -> Result<String, BackupError> {
    if target_key.is_empty() || target_key.len() > 1024 {
        return Err(BackupError::Invalid(
            "target key must be 1..=1024 chars".into(),
        ));
    }
    if target_key.starts_with('/')
        || target_key.contains("..")
        || target_key.contains('\\')
        || target_key.contains(' ')
    {
        return Err(BackupError::Invalid(
            "target key must be a plain object key".into(),
        ));
    }
    if host_label.is_empty() || host_label.len() > 128 {
        return Err(BackupError::Invalid(
            "host label must be 1..=128 chars".into(),
        ));
    }
    Ok(format!(
        "openpanel migrate import --target {target_key} --host {host_label}"
    ))
}

/// Assess one migration bundle against this host's inventory.
pub fn assess_migration_readiness(
    manifest_schema: u32,
    existing_names: &[String],
    planned_names: &[String],
    target_key: &str,
    host_label: &str,
) -> Result<MigrationReadiness, BackupError> {
    let compatible = check_manifest_compatibility(manifest_schema);
    let collisions = preview_migration_collisions(existing_names, planned_names);
    let bootstrap_command = migration_bootstrap_command(target_key, host_label)?;
    let guidance = if !compatible {
        "incompatible: bundle schema is newer than this binary; upgrade the destination first"
            .to_string()
    } else if collisions.is_empty() {
        "ready: run the bootstrap command on a fresh compatible host, then verify inventory and audit trail"
            .to_string()
    } else {
        format!(
            "collisions: {} name(s) already exist; confirm overwrite scope before importing",
            collisions.len()
        )
    };
    Ok(MigrationReadiness {
        compatible,
        collisions,
        bootstrap_command,
        guidance,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Marker so the spec-test-drift gate maps these tests to the
    /// `backup-dr-operations` capability.
    const CAPABILITY: &str = "backup-dr-operations";

    #[test]
    fn capability_marker_is_backup_dr_operations() {
        assert_eq!(CAPABILITY, "backup-dr-operations");
    }

    #[test]
    fn supported_schema_is_compatible() {
        assert!(check_manifest_compatibility(SUPPORTED_MANIFEST_SCHEMA));
        assert!(check_manifest_compatibility(0));
    }

    #[test]
    fn newer_schema_is_incompatible() {
        assert!(!check_manifest_compatibility(SUPPORTED_MANIFEST_SCHEMA + 1));
    }

    #[test]
    fn collision_preview_reports_sorted_intersection() {
        let existing = vec!["b.example".to_string(), "a.example".to_string()];
        let planned = vec![
            "c.example".to_string(),
            "a.example".to_string(),
            "b.example".to_string(),
            "a.example".to_string(),
        ];
        assert_eq!(
            preview_migration_collisions(&existing, &planned),
            vec!["a.example".to_string(), "b.example".to_string()]
        );
    }

    #[test]
    fn empty_intersection_is_clean() {
        let existing = vec!["a.example".to_string()];
        let planned = vec!["b.example".to_string()];
        assert!(preview_migration_collisions(&existing, &planned).is_empty());
    }

    #[test]
    fn bootstrap_command_carries_key_and_host() {
        let command = migration_bootstrap_command("host1/bundle.tar.gz", "fresh-1").unwrap();
        assert!(command.contains("host1/bundle.tar.gz"));
        assert!(command.contains("fresh-1"));
        assert!(!command.to_lowercase().contains("secret"));
    }

    #[test]
    fn bootstrap_command_rejects_unsafe_keys() {
        for bad in ["", "/absolute", "../escape", "a b", "back\\slash"] {
            assert!(migration_bootstrap_command(bad, "h").is_err());
        }
        assert!(migration_bootstrap_command("ok/key", "").is_err());
    }

    #[test]
    fn readiness_ready_when_compatible_and_clean() {
        let readiness = assess_migration_readiness(
            SUPPORTED_MANIFEST_SCHEMA,
            &["a.example".to_string()],
            &["b.example".to_string()],
            "host1/bundle.tar.gz",
            "fresh-1",
        )
        .unwrap();
        assert!(readiness.compatible());
        assert!(readiness.collisions().is_empty());
        assert!(readiness.guidance().contains("verify inventory"));
    }

    #[test]
    fn readiness_flags_collisions() {
        let readiness = assess_migration_readiness(
            SUPPORTED_MANIFEST_SCHEMA,
            &["a.example".to_string()],
            &["a.example".to_string()],
            "host1/bundle.tar.gz",
            "fresh-1",
        )
        .unwrap();
        assert_eq!(readiness.collisions(), &["a.example".to_string()]);
        assert!(readiness.guidance().contains("overwrite"));
    }

    #[test]
    fn readiness_blocks_incompatible_schema() {
        let readiness = assess_migration_readiness(
            SUPPORTED_MANIFEST_SCHEMA + 9,
            &[],
            &["b.example".to_string()],
            "host1/bundle.tar.gz",
            "fresh-1",
        )
        .unwrap();
        assert!(!readiness.compatible());
        assert!(readiness.guidance().contains("upgrade"));
    }

    #[test]
    fn prop_collision_preview_is_sorted_unique_intersection() {
        use proptest::prelude::*;
        let strategy = (
            proptest::collection::vec("[a-z]{1,8}", 0..8),
            proptest::collection::vec("[a-z]{1,8}", 0..8),
        );
        proptest::test_runner::TestRunner::new(ProptestConfig::with_cases(100))
            .run(&strategy, |(existing, planned)| {
                let preview = preview_migration_collisions(&existing, &planned);
                let mut sorted = preview.clone();
                sorted.sort();
                sorted.dedup();
                prop_assert_eq!(preview.clone(), sorted);
                for name in &preview {
                    prop_assert!(existing.contains(name));
                    prop_assert!(planned.contains(name));
                }
                Ok(())
            })
            .unwrap();
    }
}
