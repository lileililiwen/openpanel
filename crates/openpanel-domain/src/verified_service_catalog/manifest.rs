//! Verified-service-catalog manifest shapes: the publisher
//! identity, the pinned source digest, the compatibility matrix,
//! the resource/permission/health/backup/upgrade declarations, and
//! the signed [`ServiceManifest`] aggregate itself. The trust /
//! policy / job shapes live in `types.rs`; validation and planning
//! helpers live in `logic.rs`.
//!
//! I/O-free. Every constructor validates its inputs so a hostile or
//! malformed manifest cannot be built into the catalog in a
//! planning-ready state.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::software_center::SoftwareVersion;
use crate::verified_service_catalog::types::VerifiedServiceCatalogError;

/// Upper bound on catalog entry and publisher ids.
pub const MAX_SERVICE_ID: usize = 64;
/// Upper bound on display names.
pub const MAX_SERVICE_NAME: usize = 128;
/// Upper bound on publisher key fingerprints.
pub const MAX_FINGERPRINT: usize = 128;
/// Upper bound on the presented signature hex length (an HMAC-SHA256
/// signature is 64 hex chars; the bound leaves room for future
/// algorithms without accepting unbounded payloads).
pub const MAX_SIGNATURE_LEN: usize = 128;

/// A service publisher identity: a stable id plus the public key
/// fingerprint used to verify manifest signatures.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublisherIdentity {
    id: String,
    name: String,
    fingerprint: String,
}

impl PublisherIdentity {
    /// Build a publisher identity. The id is a software-center-style
    /// package identifier (`[a-z0-9][a-z0-9._-]*`); the name and
    /// fingerprint must be non-empty and bounded.
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        fingerprint: impl Into<String>,
    ) -> Result<Self, VerifiedServiceCatalogError> {
        let id = id.into();
        let name = name.into();
        let fingerprint = fingerprint.into();
        validate_service_id(&id)?;
        if name.trim().is_empty() || name.len() > MAX_SERVICE_NAME {
            return Err(VerifiedServiceCatalogError::Invalid(
                "publisher name must be 1..=128 characters".into(),
            ));
        }
        if !fingerprint.is_ascii()
            || fingerprint.trim().is_empty()
            || fingerprint.len() > MAX_FINGERPRINT
        {
            return Err(VerifiedServiceCatalogError::Invalid(
                "fingerprint must be non-empty ASCII, <= 128 characters".into(),
            ));
        }
        Ok(Self {
            id,
            name,
            fingerprint,
        })
    }

    /// Stable publisher id.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Display name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Public key fingerprint (HMAC key material for signature
    /// verification in this conformance model).
    pub fn fingerprint(&self) -> &str {
        &self.fingerprint
    }
}

/// The pinned, immutable source digest a manifest ships from. Only
/// `sha256:<64 lowercase hex>` is accepted — mutable tags (
/// `latest`, branches) are rejected at the type level.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SourceDigest(String);

impl SourceDigest {
    /// Validate and build a source digest.
    pub fn new(value: impl Into<String>) -> Result<Self, VerifiedServiceCatalogError> {
        let value = value.into();
        let valid = match value.strip_prefix("sha256:") {
            Some(hex) => {
                hex.len() == 64
                    && hex
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            }
            None => false,
        };
        if !valid {
            return Err(VerifiedServiceCatalogError::Invalid(
                "source digest must be sha256:<64 lowercase hex>".into(),
            ));
        }
        Ok(Self(value))
    }

    /// Digest text (`sha256:...`).
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// OS/architecture targets a service template declares compatible.
/// Each entry is `<os-id>:<arch>` where either segment may be the
/// `*` wildcard (e.g. `debian:amd64`, `ubuntu:*`, `*:*`). The
/// segment grammar mirrors the portable-runtime target manifest
/// (`packages/installer/target-manifest.txt`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceCompatibility {
    targets: BTreeSet<String>,
}

impl ServiceCompatibility {
    /// Validate and build a compatibility set. Empty sets and
    /// malformed target strings are rejected.
    pub fn new(targets: Vec<String>) -> Result<Self, VerifiedServiceCatalogError> {
        if targets.is_empty() {
            return Err(VerifiedServiceCatalogError::Invalid(
                "compatibility must declare at least one target".into(),
            ));
        }
        let mut set = BTreeSet::new();
        for target in targets {
            let Some((os, arch)) = target.split_once(':') else {
                return Err(VerifiedServiceCatalogError::Invalid(format!(
                    "target `{target}` must be <os>:<arch>"
                )));
            };
            for segment in [os, arch] {
                let ok = !segment.is_empty()
                    && segment.len() <= 32
                    && segment.bytes().all(|byte| {
                        byte == b'*'
                            || byte.is_ascii_lowercase()
                            || byte.is_ascii_digit()
                            || matches!(byte, b'.' | b'_' | b'-')
                    });
                if !ok {
                    return Err(VerifiedServiceCatalogError::Invalid(format!(
                        "target `{target}` has an invalid segment"
                    )));
                }
            }
            set.insert(target);
        }
        Ok(Self { targets: set })
    }

    /// Declared target strings (sorted).
    pub fn targets(&self) -> impl Iterator<Item = &str> {
        self.targets.iter().map(String::as_str)
    }

    /// Whether the given (os, arch) pair is covered.
    pub fn supports(&self, os: &str, arch: &str) -> bool {
        self.targets.contains("*:*")
            || self.targets.contains(&format!("{os}:*"))
            || self.targets.contains(&format!("*:{arch}"))
            || self.targets.contains(&format!("{os}:{arch}"))
    }
}

/// CPU / memory / storage the service requires. All axes must be
/// positive so a template cannot smuggle a zero-cost resource
/// claim into a quota check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceRequest {
    cpu_millis: u32,
    memory_bytes: u64,
    storage_bytes: u64,
}

impl ResourceRequest {
    /// Validate and build a resource request.
    pub fn new(
        cpu_millis: u32,
        memory_bytes: u64,
        storage_bytes: u64,
    ) -> Result<Self, VerifiedServiceCatalogError> {
        if cpu_millis == 0 || memory_bytes == 0 || storage_bytes == 0 {
            return Err(VerifiedServiceCatalogError::Invalid(
                "resource axes must be positive".into(),
            ));
        }
        Ok(Self {
            cpu_millis,
            memory_bytes,
            storage_bytes,
        })
    }

    /// Required CPU in millicores.
    pub fn cpu_millis(&self) -> u32 {
        self.cpu_millis
    }

    /// Required memory in bytes.
    pub fn memory_bytes(&self) -> u64 {
        self.memory_bytes
    }

    /// Required storage in bytes.
    pub fn storage_bytes(&self) -> u64 {
        self.storage_bytes
    }
}

/// Declared network, port, credential, and data effects. Credential
/// material is referenced by identifier only — a manifest never
/// carries a secret value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServicePermissions {
    public_network: bool,
    ports: Vec<u16>,
    credential_refs: Vec<String>,
    data_effects: Vec<String>,
}

impl ServicePermissions {
    /// Validate and build a permission declaration. Ports must be
    /// in `1..=65535` and unique; refs and effects must be
    /// non-empty tokens.
    pub fn new(
        public_network: bool,
        ports: Vec<u16>,
        credential_refs: Vec<String>,
        data_effects: Vec<String>,
    ) -> Result<Self, VerifiedServiceCatalogError> {
        let mut seen = BTreeSet::new();
        for port in &ports {
            if *port == 0 {
                return Err(VerifiedServiceCatalogError::Invalid(
                    "port 0 is not allowed".into(),
                ));
            }
            if !seen.insert(port) {
                return Err(VerifiedServiceCatalogError::Invalid(format!(
                    "duplicate port {port}"
                )));
            }
        }
        for token in credential_refs.iter().chain(data_effects.iter()) {
            if token.trim().is_empty()
                || token.len() > 128
                || !token.is_ascii()
                || token.chars().any(|c| c.is_whitespace() || c == '\0')
            {
                return Err(VerifiedServiceCatalogError::Invalid(format!(
                    "`{token}` is not a valid effect/credential token"
                )));
            }
        }
        Ok(Self {
            public_network,
            ports,
            credential_refs,
            data_effects,
        })
    }

    /// Whether the service exposes a public listener.
    pub fn public_network(&self) -> bool {
        self.public_network
    }

    /// Declared ports (in declaration order).
    pub fn ports(&self) -> &[u16] {
        &self.ports
    }

    /// Credential references (ids only, never values).
    pub fn credential_refs(&self) -> &[String] {
        &self.credential_refs
    }

    /// Declared data effects (e.g. `writes:/var/lib/demo`).
    pub fn data_effects(&self) -> &[String] {
        &self.data_effects
    }
}

/// Health probe contract for a service. `max_attempts` bounds the
/// health-verification retries so a failed rollout can never loop
/// unboundedly.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceHealthProbe {
    path: String,
    interval_seconds: u32,
    max_attempts: u32,
}

impl ServiceHealthProbe {
    /// Validate and build a health probe declaration.
    pub fn new(
        path: impl Into<String>,
        interval_seconds: u32,
        max_attempts: u32,
    ) -> Result<Self, VerifiedServiceCatalogError> {
        let path = path.into();
        if !path.starts_with('/') || path.len() < 2 || path.len() > 256 {
            return Err(VerifiedServiceCatalogError::Invalid(
                "health path must start with `/` and be 2..=256 chars".into(),
            ));
        }
        if !(1..=3600).contains(&interval_seconds) || !(1..=10).contains(&max_attempts) {
            return Err(VerifiedServiceCatalogError::Invalid(
                "health interval must be 1..=3600 and attempts 1..=10".into(),
            ));
        }
        Ok(Self {
            path,
            interval_seconds,
            max_attempts,
        })
    }

    /// Probe path on the service listener.
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Seconds between probes.
    pub fn interval_seconds(&self) -> u32 {
        self.interval_seconds
    }

    /// Bounded probe attempts before declaring failure.
    pub fn max_attempts(&self) -> u32 {
        self.max_attempts
    }
}

/// Backup contract: whether a pre-upgrade backup is required.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceBackupPolicy {
    required_before_upgrade: bool,
}

impl ServiceBackupPolicy {
    /// Build a backup declaration.
    pub fn new(required_before_upgrade: bool) -> Self {
        Self {
            required_before_upgrade,
        }
    }

    /// Whether a backup job must succeed before an upgrade plans.
    pub fn required_before_upgrade(&self) -> bool {
        self.required_before_upgrade
    }
}

/// Upgrade contract: bounded attempts and whether a failed upgrade
/// must roll back to the previous verified release.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceUpgradePolicy {
    max_attempts: u32,
    rollback_on_failure: bool,
}

impl ServiceUpgradePolicy {
    /// Validate and build an upgrade policy.
    pub fn new(
        max_attempts: u32,
        rollback_on_failure: bool,
    ) -> Result<Self, VerifiedServiceCatalogError> {
        if !(1..=10).contains(&max_attempts) {
            return Err(VerifiedServiceCatalogError::Invalid(
                "upgrade attempts must be 1..=10".into(),
            ));
        }
        Ok(Self {
            max_attempts,
            rollback_on_failure,
        })
    }

    /// Bounded upgrade attempts.
    pub fn max_attempts(&self) -> u32 {
        self.max_attempts
    }

    /// Whether a failed upgrade restores the previous release.
    pub fn rollback_on_failure(&self) -> bool {
        self.rollback_on_failure
    }
}

/// The signed, declarative unit of catalog contribution. The
/// manifest carries publisher, pinned source digest, compatibility,
/// permissions, resources, health, backup, and upgrade metadata —
/// never opaque scripts and never secret values.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceManifest {
    id: String,
    name: String,
    version: String,
    publisher: PublisherIdentity,
    digest: SourceDigest,
    signature: String,
    compatibility: ServiceCompatibility,
    resources: ResourceRequest,
    permissions: ServicePermissions,
    health: ServiceHealthProbe,
    backup_policy: ServiceBackupPolicy,
    upgrade_policy: ServiceUpgradePolicy,
}

impl ServiceManifest {
    /// Validate and build a manifest. Schema-level validation lives
    /// here; signature *verification* is a separate step
    /// (`verify_manifest_signature`) because a hostile manifest must
    /// remain reviewable while being blocked.
    #[allow(clippy::too_many_arguments)] // mirrors WebApplicationManifest; the manifest is a flat declarative record
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        version: impl Into<String>,
        publisher: PublisherIdentity,
        digest: SourceDigest,
        signature: impl Into<String>,
        compatibility: ServiceCompatibility,
        resources: ResourceRequest,
        permissions: ServicePermissions,
        health: ServiceHealthProbe,
        backup_policy: ServiceBackupPolicy,
        upgrade_policy: ServiceUpgradePolicy,
    ) -> Result<Self, VerifiedServiceCatalogError> {
        let id = id.into();
        let name = name.into();
        let version = version.into();
        validate_service_id(&id)?;
        if name.trim().is_empty() || name.len() > MAX_SERVICE_NAME {
            return Err(VerifiedServiceCatalogError::Invalid(
                "name must be 1..=128 characters".into(),
            ));
        }
        // Reuse the software-center version grammar: `major.minor`
        // or `major.minor.patch`, no leading zeros.
        SoftwareVersion::parse(&version).map_err(|_| {
            VerifiedServiceCatalogError::Invalid(format!(
                "version `{version}` is not numeric semver"
            ))
        })?;
        let signature = signature.into();
        if signature.is_empty() || signature.len() > MAX_SIGNATURE_LEN || !signature.is_ascii() {
            return Err(VerifiedServiceCatalogError::Invalid(
                "signature must be non-empty ASCII, <= 128 chars".into(),
            ));
        }
        Ok(Self {
            id,
            name,
            version,
            publisher,
            digest,
            signature,
            compatibility,
            resources,
            permissions,
            health,
            backup_policy,
            upgrade_policy,
        })
    }

    /// Stable catalog entry id.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Display name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Manifest version (software-center semver grammar).
    pub fn version(&self) -> &str {
        &self.version
    }

    /// Declared publisher identity.
    pub fn publisher(&self) -> &PublisherIdentity {
        &self.publisher
    }

    /// Pinned, immutable source digest.
    pub fn digest(&self) -> &SourceDigest {
        &self.digest
    }

    /// Presented signature (hex). Verified separately against the
    /// publisher key.
    pub fn signature(&self) -> &str {
        &self.signature
    }

    /// Declared compatibility matrix.
    pub fn compatibility(&self) -> &ServiceCompatibility {
        &self.compatibility
    }

    /// Declared resources.
    pub fn resources(&self) -> &ResourceRequest {
        &self.resources
    }

    /// Declared permissions and effects.
    pub fn permissions(&self) -> &ServicePermissions {
        &self.permissions
    }

    /// Declared health probe.
    pub fn health(&self) -> &ServiceHealthProbe {
        &self.health
    }

    /// Declared backup policy.
    pub fn backup_policy(&self) -> &ServiceBackupPolicy {
        &self.backup_policy
    }

    /// Declared upgrade policy.
    pub fn upgrade_policy(&self) -> &ServiceUpgradePolicy {
        &self.upgrade_policy
    }
}

fn validate_service_id(id: &str) -> Result<(), VerifiedServiceCatalogError> {
    let ok = !id.is_empty()
        && id.len() <= MAX_SERVICE_ID
        && id.is_ascii()
        && id
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        && id.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
        });
    if ok {
        Ok(())
    } else {
        Err(VerifiedServiceCatalogError::Invalid(format!(
            "`{id}` is not a valid service id ([a-z0-9][a-z0-9._-]*, <= 64)"
        )))
    }
}
