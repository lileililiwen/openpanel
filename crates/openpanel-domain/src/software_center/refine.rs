//! Software-center refinement: typed `ComponentKind` and
//! `WebApplicationManifest` value objects the per-resource
//! installer consumes.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Classification of a catalog entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComponentKind {
    /// System component (Nginx, PHP-FPM, MySQL, …).
    System,
    /// User-facing web application (WordPress, Ghost, NextCloud, …).
    Application,
}

/// Whether an `Application` may be installed once per site or
/// once per panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetSiteType {
    /// One instance per site (typical CMS install).
    SingleSite,
    /// One instance shared across sites (NextCloud, etc.).
    MultiTenant,
}

/// A typed manifest for a web application.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebApplicationManifest {
    /// Stable catalog id.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Version.
    pub version: String,
    /// Target site type.
    pub target_site_type: TargetSiteType,
    /// Whether the application requires a database.
    pub requires_db: bool,
    /// Whether the application requires PHP.
    pub requires_php: bool,
    /// Minimum storage bytes the site must have available.
    pub requires_storage_bytes: u64,
    /// Install signature.
    pub install_signature: String,
}

impl WebApplicationManifest {
    /// Build a new manifest. The id, name, and version must be
    /// non-empty; the storage requirement must be 0+ bytes.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        version: impl Into<String>,
        target_site_type: TargetSiteType,
        requires_db: bool,
        requires_php: bool,
        requires_storage_bytes: u64,
        install_signature: impl Into<String>,
    ) -> Result<Self, SoftwareCenterRefineError> {
        let id = id.into();
        let name = name.into();
        let version = version.into();
        if id.is_empty() || name.is_empty() || version.is_empty() {
            return Err(SoftwareCenterRefineError::InvalidManifest);
        }
        Ok(Self {
            id,
            name,
            version,
            target_site_type,
            requires_db,
            requires_php,
            requires_storage_bytes,
            install_signature: install_signature.into(),
        })
    }

    /// Restore from persistence.
    #[allow(clippy::too_many_arguments)]
    pub fn restore(
        id: String,
        name: String,
        version: String,
        target_site_type: TargetSiteType,
        requires_db: bool,
        requires_php: bool,
        requires_storage_bytes: u64,
        install_signature: String,
    ) -> Self {
        Self {
            id,
            name,
            version,
            target_site_type,
            requires_db,
            requires_php,
            requires_storage_bytes,
            install_signature,
        }
    }

    /// Stable catalog id.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Display name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Version.
    pub fn version(&self) -> &str {
        &self.version
    }

    /// Target site type.
    pub fn target_site_type(&self) -> TargetSiteType {
        self.target_site_type
    }

    /// Whether the application requires a database.
    pub fn requires_db(&self) -> bool {
        self.requires_db
    }

    /// Whether the application requires PHP.
    pub fn requires_php(&self) -> bool {
        self.requires_php
    }

    /// Minimum storage bytes the site must have available.
    pub fn requires_storage_bytes(&self) -> u64 {
        self.requires_storage_bytes
    }

    /// Install signature.
    pub fn install_signature(&self) -> &str {
        &self.install_signature
    }
}

/// The catalog entry. The catalog keeps two separate indices
/// (system and application) and the install dispatcher refuses
/// to install across kinds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogEntry {
    /// The catalog entry kind.
    pub kind: ComponentKind,
    /// Stable package id.
    pub package_id: String,
    /// Display name.
    pub name: String,
    /// Version.
    pub version: String,
    /// Human-readable description.
    pub description: String,
}

impl CatalogEntry {
    /// Build a new entry.
    pub fn new(
        kind: ComponentKind,
        package_id: impl Into<String>,
        name: impl Into<String>,
        version: impl Into<String>,
        description: impl Into<String>,
    ) -> Result<Self, SoftwareCenterRefineError> {
        let package_id = package_id.into();
        let name = name.into();
        let version = version.into();
        if package_id.is_empty() || name.is_empty() || version.is_empty() {
            return Err(SoftwareCenterRefineError::InvalidEntry);
        }
        Ok(Self {
            kind,
            package_id,
            name,
            version,
            description: description.into(),
        })
    }

    /// The catalog entry kind.
    pub fn kind(&self) -> ComponentKind {
        self.kind
    }

    /// Stable package id.
    pub fn package_id(&self) -> &str {
        &self.package_id
    }
}

/// Errors that can occur in the software-center refinement.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum SoftwareCenterRefineError {
    /// The manifest is malformed.
    #[error("invalid web application manifest")]
    InvalidManifest,
    /// The catalog entry is malformed.
    #[error("invalid catalog entry")]
    InvalidEntry,
    /// An install was attempted with the wrong adapter for the
    /// entry's kind.
    #[error("install adapter mismatch: entry is {entry:?}, adapter is {adapter:?}")]
    InstallKindMismatch {
        /// The entry's kind.
        entry: ComponentKind,
        /// The adapter's kind.
        adapter: ComponentKind,
    },
}

/// Reject an install when the entry's kind does not match the
/// adapter's kind.
pub fn check_install_kind(
    entry: ComponentKind,
    adapter: ComponentKind,
) -> Result<(), SoftwareCenterRefineError> {
    if entry != adapter {
        return Err(SoftwareCenterRefineError::InstallKindMismatch { entry, adapter });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_rejects_empty_inputs() {
        let err = WebApplicationManifest::new(
            "",
            "WordPress",
            "6.0",
            TargetSiteType::SingleSite,
            true,
            true,
            1024,
            "sig",
        )
        .expect_err("must reject");
        assert_eq!(err, SoftwareCenterRefineError::InvalidManifest);
    }

    #[test]
    fn manifest_accepts_valid_payload() {
        let m = WebApplicationManifest::new(
            "wp",
            "WordPress",
            "6.0",
            TargetSiteType::SingleSite,
            true,
            true,
            1024,
            "sig",
        )
        .unwrap();
        assert!(m.requires_db());
        assert!(m.requires_php());
    }

    #[test]
    fn entry_rejects_empty_package_id() {
        let err = CatalogEntry::new(ComponentKind::Application, "", "WordPress", "6.0", "blog")
            .expect_err("must reject");
        assert_eq!(err, SoftwareCenterRefineError::InvalidEntry);
    }

    #[test]
    fn install_kind_check_rejects_mismatch() {
        let err = check_install_kind(ComponentKind::System, ComponentKind::Application)
            .expect_err("must reject");
        assert!(matches!(
            err,
            SoftwareCenterRefineError::InstallKindMismatch { .. }
        ));
    }

    #[test]
    fn install_kind_check_accepts_match() {
        assert!(check_install_kind(ComponentKind::System, ComponentKind::System).is_ok());
        assert!(check_install_kind(ComponentKind::Application, ComponentKind::Application).is_ok());
    }
}
