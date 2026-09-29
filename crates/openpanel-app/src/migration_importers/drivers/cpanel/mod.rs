//! cPanel pkgacct and legacy backup drivers.

#![allow(missing_docs)]

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use openpanel_domain::{
    DriverKind, ImportedResource, ImportedResourceKind, MigrationDriver, MigrationError,
    MigrationPlan, MigrationPlanId, MigrationRunId, MigrationWarning, PlannedResource,
};
use uuid::Uuid;

use super::ManifestTranslator;

#[derive(Debug, Clone)]
pub struct CpanelPkgacctBundle {
    pub users: Vec<String>,
    pub domains: Vec<String>,
    pub raw: Vec<u8>,
}

impl CpanelPkgacctBundle {
    pub fn new(users: Vec<String>, domains: Vec<String>) -> Self {
        Self {
            users,
            domains,
            raw: b"cpanel-pkgacct".to_vec(),
        }
    }
}

pub struct CpanelPkgacctDriver<T> {
    translator: T,
}

impl<T> CpanelPkgacctDriver<T> {
    pub fn new(translator: T) -> Self {
        Self { translator }
    }
}

#[async_trait]
impl<T: ManifestTranslator> MigrationDriver for CpanelPkgacctDriver<T> {
    type Source = CpanelPkgacctBundle;
    fn sniff(&self, source: &Self::Source) -> Option<DriverKind> {
        if source.raw.starts_with(b"cpanel-pkgacct") {
            Some(DriverKind::CpanelPkgacct)
        } else {
            None
        }
    }
    async fn dry_run(&self, source: &Self::Source) -> Result<MigrationPlan, MigrationError> {
        let mut resources = Vec::new();
        for user in &source.users {
            resources.push(PlannedResource::new(
                ImportedResourceKind::User,
                user.clone(),
                format!("user {user}"),
                64,
            )?);
        }
        for domain in &source.domains {
            resources.push(PlannedResource::new(
                ImportedResourceKind::Site,
                domain.clone(),
                format!("site {domain}"),
                128,
            )?);
        }
        Ok(MigrationPlan::new(
            MigrationPlanId::new(),
            DriverKind::CpanelPkgacct,
            resources,
            Vec::new(),
            Vec::new(),
            Utc::now(),
        ))
    }
    async fn run(
        &self,
        source: &Self::Source,
        plan: &MigrationPlan,
        run_id: MigrationRunId,
        _owner: Uuid,
        _at: DateTime<Utc>,
    ) -> Result<Vec<ImportedResource>, MigrationError> {
        let mut imported = Vec::new();
        for r in plan.resources() {
            let kind_str = format!("{:?}", r.kind);
            let _ = &source.users;
            if let Some(id) = self
                .translator
                .translate(r.kind, &r.source_key, kind_str.as_bytes())
                .map_err(MigrationError::TargetConflict)?
            {
                imported.push(ImportedResource::new(
                    run_id,
                    r.kind,
                    r.source_key.clone(),
                    id,
                ));
            }
        }
        Ok(imported)
    }
    async fn rollback(
        &self,
        imported: &[ImportedResource],
        _run_id: MigrationRunId,
        _at: DateTime<Utc>,
    ) -> Result<Vec<ImportedResource>, MigrationError> {
        Ok(imported.to_vec())
    }
}

#[derive(Debug, Clone)]
pub struct CpanelLegacyBundle {
    pub domains: Vec<String>,
    pub raw: Vec<u8>,
}

impl CpanelLegacyBundle {
    pub fn new(domains: Vec<String>) -> Self {
        Self {
            domains,
            raw: b"cpanel-legacy".to_vec(),
        }
    }
}

pub struct CpanelLegacyBackupDriver<T> {
    translator: T,
}

impl<T> CpanelLegacyBackupDriver<T> {
    pub fn new(translator: T) -> Self {
        Self { translator }
    }
}

#[async_trait]
impl<T: ManifestTranslator> MigrationDriver for CpanelLegacyBackupDriver<T> {
    type Source = CpanelLegacyBundle;
    fn sniff(&self, source: &Self::Source) -> Option<DriverKind> {
        if source.raw.starts_with(b"cpanel-legacy") {
            Some(DriverKind::CpanelLegacyBackup)
        } else {
            None
        }
    }
    async fn dry_run(&self, source: &Self::Source) -> Result<MigrationPlan, MigrationError> {
        let mut resources = Vec::new();
        let mut warnings = Vec::new();
        for domain in &source.domains {
            resources.push(PlannedResource::new(
                ImportedResourceKind::Site,
                domain.clone(),
                format!("site {domain}"),
                64,
            )?);
        }
        warnings.push(MigrationWarning {
            message: "legacy format: cron jobs not imported".to_string(),
        });
        Ok(MigrationPlan::new(
            MigrationPlanId::new(),
            DriverKind::CpanelLegacyBackup,
            resources,
            Vec::new(),
            warnings,
            Utc::now(),
        ))
    }
    async fn run(
        &self,
        source: &Self::Source,
        plan: &MigrationPlan,
        run_id: MigrationRunId,
        _owner: Uuid,
        _at: DateTime<Utc>,
    ) -> Result<Vec<ImportedResource>, MigrationError> {
        let mut imported = Vec::new();
        for r in plan.resources() {
            let _ = &source.domains;
            if let Some(id) = self
                .translator
                .translate(r.kind, &r.source_key, b"legacy")
                .map_err(MigrationError::TargetConflict)?
            {
                imported.push(ImportedResource::new(
                    run_id,
                    r.kind,
                    r.source_key.clone(),
                    id,
                ));
            }
        }
        Ok(imported)
    }
    async fn rollback(
        &self,
        imported: &[ImportedResource],
        _run_id: MigrationRunId,
        _at: DateTime<Utc>,
    ) -> Result<Vec<ImportedResource>, MigrationError> {
        Ok(imported.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::migration_importers::RefuseAll;

    #[test]
    fn pkgacct_sniff_recognises_format() {
        let driver = CpanelPkgacctDriver::new(RefuseAll);
        let bundle =
            CpanelPkgacctBundle::new(vec!["alice".to_string()], vec!["example.com".to_string()]);
        assert_eq!(driver.sniff(&bundle), Some(DriverKind::CpanelPkgacct));
        let bad = CpanelPkgacctBundle {
            users: vec![],
            domains: vec![],
            raw: b"other".to_vec(),
        };
        assert_eq!(driver.sniff(&bad), None);
    }

    #[tokio::test]
    async fn pkgacct_dry_run_lists_users_and_domains() {
        let driver = CpanelPkgacctDriver::new(RefuseAll);
        let bundle =
            CpanelPkgacctBundle::new(vec!["alice".to_string()], vec!["example.com".to_string()]);
        let plan = driver.dry_run(&bundle).await.unwrap();
        assert_eq!(plan.resources().len(), 2);
    }

    #[test]
    fn legacy_sniff_recognises_format() {
        let driver = CpanelLegacyBackupDriver::new(RefuseAll);
        let bundle = CpanelLegacyBundle::new(vec!["example.com".to_string()]);
        assert_eq!(driver.sniff(&bundle), Some(DriverKind::CpanelLegacyBackup));
    }

    #[tokio::test]
    async fn legacy_dry_run_marks_unsupported_as_warning() {
        let driver = CpanelLegacyBackupDriver::new(RefuseAll);
        let bundle = CpanelLegacyBundle::new(vec!["example.com".to_string()]);
        let plan = driver.dry_run(&bundle).await.unwrap();
        assert!(!plan.warnings().is_empty());
    }
}
