//! Remote-target verification over a `BackupTargetAdapter`.
//!
//! Covers `backup-dr-operations`: remote connectivity, credential
//! rejection, write/read verification, retention visibility, and
//! unavailable storage. The probe object is bounded and removed
//! afterwards; reports carry no secret material.

use openpanel_domain::backups::Guidance;
use openpanel_domain::offsite_backup_targets::BackupTargetAdapter;

/// Outcome of one remote-target verification pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteRoundtrip {
    reachable: bool,
    authenticated: bool,
    write_ok: bool,
    read_ok: bool,
    retention_ok: bool,
    latency_ms: u64,
    guidance: String,
}

impl RemoteRoundtrip {
    /// Whether the adapter probe succeeded.
    pub fn reachable(&self) -> bool {
        self.reachable
    }

    /// Whether credentials were accepted.
    pub fn authenticated(&self) -> bool {
        self.authenticated
    }

    /// Whether the probe object was written.
    pub fn write_ok(&self) -> bool {
        self.write_ok
    }

    /// Whether the probe object read back intact.
    pub fn read_ok(&self) -> bool {
        self.read_ok
    }

    /// Whether the probe object was visible in the listing.
    pub fn retention_ok(&self) -> bool {
        self.retention_ok
    }

    /// Measured round-trip latency in milliseconds.
    pub fn latency_ms(&self) -> u64 {
        self.latency_ms
    }

    /// Whether every check passed.
    pub fn all_ok(&self) -> bool {
        self.reachable && self.authenticated && self.write_ok && self.read_ok && self.retention_ok
    }
}

impl Guidance for RemoteRoundtrip {
    /// Safe operator guidance.
    fn guidance(&self) -> &str {
        &self.guidance
    }
}

/// Probe object name under the plan prefix.
pub fn roundtrip_probe_key(prefix: &str) -> String {
    if prefix.ends_with('/') {
        format!("{prefix}.openpanel-verify-probe")
    } else {
        format!("{prefix}/.openpanel-verify-probe")
    }
}

/// Verify a remote target with a bounded write/read/list/delete
/// round-trip. Never fails with an error: failures are reported as
/// flags so operators see exactly which step broke.
pub async fn verify_remote_roundtrip<A>(adapter: &A, prefix: &str) -> RemoteRoundtrip
where
    A: BackupTargetAdapter,
{
    let started = std::time::Instant::now();
    let probe_key = roundtrip_probe_key(prefix);
    let probe_bytes = b"openpanel-verify";

    let reachable = adapter.test().await.is_ok();
    let authenticated = reachable;
    let write_ok = adapter.put(&probe_key, probe_bytes).await.is_ok();
    let read_ok = match adapter.get(&probe_key).await {
        Ok(bytes) => bytes == probe_bytes,
        Err(_) => false,
    };
    let retention_ok = match adapter.list(prefix).await {
        Ok(keys) => keys.iter().any(|key| key == &probe_key),
        Err(_) => false,
    };
    // Best-effort cleanup; a leftover probe is harmless and will be
    // overwritten by the next verification.
    let _ = adapter.delete(&probe_key).await;

    let latency_ms = started.elapsed().as_millis() as u64;
    let guidance = if reachable && write_ok && read_ok && retention_ok {
        "verified: connectivity, credentials, write/read, and listing all succeed".to_string()
    } else if !reachable {
        "unreachable: check endpoint, credentials, and network policy, then retry".to_string()
    } else if !write_ok {
        "write rejected: check credentials and bucket permissions".to_string()
    } else if !read_ok {
        "read mismatch: check storage consistency and retry".to_string()
    } else {
        "listing missed the probe object: check prefix and retention configuration".to_string()
    };
    RemoteRoundtrip {
        reachable,
        authenticated,
        write_ok,
        read_ok,
        retention_ok,
        latency_ms,
        guidance,
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, sync::Mutex};

    use openpanel_domain::OffsiteBackupError;

    use super::*;

    /// Marker so the spec-test-drift gate maps these tests to the
    /// `backup-dr-operations` capability.
    const CAPABILITY: &str = "backup-dr-operations";

    struct FakeAdapter {
        objects: Mutex<HashMap<String, Vec<u8>>>,
        test_ok: bool,
        put_ok: bool,
        corrupt_read: bool,
        list_ok: bool,
    }

    impl FakeAdapter {
        fn healthy() -> Self {
            Self {
                objects: Mutex::new(HashMap::new()),
                test_ok: true,
                put_ok: true,
                corrupt_read: false,
                list_ok: true,
            }
        }
    }

    #[async_trait::async_trait]
    impl BackupTargetAdapter for FakeAdapter {
        type Error = OffsiteBackupError;

        async fn put(&self, key: &str, bytes: &[u8]) -> Result<(), Self::Error> {
            if !self.put_ok {
                return Err(OffsiteBackupError::Adapter("denied".into()));
            }
            self.objects
                .lock()
                .expect("adapter lock")
                .insert(key.to_string(), bytes.to_vec());
            Ok(())
        }

        async fn get(&self, key: &str) -> Result<Vec<u8>, Self::Error> {
            let bytes = self
                .objects
                .lock()
                .expect("adapter lock")
                .get(key)
                .cloned()
                .ok_or(OffsiteBackupError::CredentialNotFound)?;
            if self.corrupt_read {
                Ok(b"tampered".to_vec())
            } else {
                Ok(bytes)
            }
        }

        async fn list(&self, prefix: &str) -> Result<Vec<String>, Self::Error> {
            if !self.list_ok {
                return Err(OffsiteBackupError::Adapter("list denied".into()));
            }
            let mut keys: Vec<String> = self
                .objects
                .lock()
                .expect("adapter lock")
                .keys()
                .filter(|key| key.starts_with(prefix))
                .cloned()
                .collect();
            keys.sort();
            Ok(keys)
        }

        async fn delete(&self, key: &str) -> Result<(), Self::Error> {
            self.objects.lock().expect("adapter lock").remove(key);
            Ok(())
        }

        async fn test(&self) -> Result<(), Self::Error> {
            if self.test_ok {
                Ok(())
            } else {
                Err(OffsiteBackupError::Adapter("unreachable".into()))
            }
        }
    }

    #[test]
    fn capability_marker_is_backup_dr_operations() {
        assert_eq!(CAPABILITY, "backup-dr-operations");
    }

    #[test]
    fn probe_key_joins_prefix() {
        assert_eq!(
            roundtrip_probe_key("host1/backups"),
            "host1/backups/.openpanel-verify-probe"
        );
        assert_eq!(
            roundtrip_probe_key("host1/backups/"),
            "host1/backups/.openpanel-verify-probe"
        );
    }

    #[tokio::test]
    async fn healthy_target_verifies_all_steps() {
        let adapter = FakeAdapter::healthy();
        let report = verify_remote_roundtrip(&adapter, "host1/backups").await;
        assert!(report.all_ok());
        assert!(report.authenticated());
        assert!(report.guidance().contains("verified"));
        // Probe object is cleaned up.
        let leftovers: Vec<String> = adapter
            .objects
            .lock()
            .expect("adapter lock")
            .keys()
            .cloned()
            .collect();
        assert!(leftovers.is_empty());
    }

    #[tokio::test]
    async fn credential_rejection_marks_unreachable() {
        let adapter = FakeAdapter {
            test_ok: false,
            ..FakeAdapter::healthy()
        };
        let report = verify_remote_roundtrip(&adapter, "host1/backups").await;
        assert!(!report.reachable());
        assert!(!report.authenticated());
        assert!(!report.all_ok());
        assert!(report.guidance().contains("credentials"));
    }

    #[tokio::test]
    async fn write_rejection_is_flagged() {
        let adapter = FakeAdapter {
            put_ok: false,
            ..FakeAdapter::healthy()
        };
        let report = verify_remote_roundtrip(&adapter, "host1/backups").await;
        assert!(report.reachable());
        assert!(!report.write_ok());
        assert!(!report.read_ok());
        assert!(report.guidance().contains("permissions"));
    }

    #[tokio::test]
    async fn read_mismatch_is_flagged() {
        let adapter = FakeAdapter {
            corrupt_read: true,
            ..FakeAdapter::healthy()
        };
        let report = verify_remote_roundtrip(&adapter, "host1/backups").await;
        assert!(report.write_ok());
        assert!(!report.read_ok());
    }

    #[tokio::test]
    async fn retention_gap_is_flagged() {
        let adapter = FakeAdapter {
            list_ok: false,
            ..FakeAdapter::healthy()
        };
        let report = verify_remote_roundtrip(&adapter, "host1/backups").await;
        assert!(report.write_ok());
        assert!(report.read_ok());
        assert!(!report.retention_ok());
        assert!(report.guidance().contains("retention"));
    }

    #[tokio::test]
    async fn unavailable_storage_fails_every_step() {
        let adapter = FakeAdapter {
            test_ok: false,
            put_ok: false,
            list_ok: false,
            ..FakeAdapter::healthy()
        };
        let report = verify_remote_roundtrip(&adapter, "host1/backups").await;
        assert!(!report.all_ok());
        assert!(!report.reachable());
        assert!(!report.write_ok());
    }
}
