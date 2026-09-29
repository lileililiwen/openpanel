//! Baota backup driver.

#![allow(missing_docs)]

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use openpanel_domain::{
    DriverKind, ImportedResource, ImportedResourceKind, MigrationDriver, MigrationError,
    MigrationPlan, MigrationPlanId, MigrationRunId, MigrationWarning, PlannedResource,
};
use uuid::Uuid;

use crate::migration_importers::ManifestTranslator;

#[derive(Debug, Clone)]
pub struct BaotaBundle {
    pub sites: Vec<String>,
    pub databases: Vec<String>,
    pub raw: Vec<u8>,
}

impl BaotaBundle {
    pub fn new(sites: Vec<String>, databases: Vec<String>) -> Self {
        Self {
            sites,
            databases,
            raw: b"baota-backup".to_vec(),
        }
    }
}

pub struct BaotaBackupDriver<T> {
    translator: T,
}

impl<T> BaotaBackupDriver<T> {
    pub fn new(translator: T) -> Self {
        Self { translator }
    }
}

#[async_trait]
impl<T: ManifestTranslator> MigrationDriver for BaotaBackupDriver<T> {
    type Source = BaotaBundle;
    fn sniff(&self, source: &Self::Source) -> Option<DriverKind> {
        if source.raw.starts_with(b"baota-backup") {
            Some(DriverKind::BaotaBackup)
        } else {
            None
        }
    }
    async fn dry_run(&self, source: &Self::Source) -> Result<MigrationPlan, MigrationError> {
        let mut resources = Vec::new();
        let mut warnings = Vec::new();
        for site in &source.sites {
            resources.push(PlannedResource::new(
                ImportedResourceKind::Site,
                site.clone(),
                format!("site {site}"),
                64,
            )?);
        }
        for db in &source.databases {
            resources.push(PlannedResource::new(
                ImportedResourceKind::Database,
                db.clone(),
                format!("database {db}"),
                32,
            )?);
        }
        warnings.push(MigrationWarning {
            message: "baota: WAF rules not imported".to_string(),
        });
        Ok(MigrationPlan::new(
            MigrationPlanId::new(),
            DriverKind::BaotaBackup,
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
            let _ = &source.sites;
            if let Some(id) = self
                .translator
                .translate(r.kind, &r.source_key, b"baota")
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
    fn baota_sniff_recognises_format() {
        let driver = BaotaBackupDriver::new(RefuseAll);
        let bundle = BaotaBundle::new(vec!["example.com".to_string()], vec!["mydb".to_string()]);
        assert_eq!(driver.sniff(&bundle), Some(DriverKind::BaotaBackup));
    }

    #[tokio::test]
    async fn baota_dry_run_lists_sites_and_databases() {
        let driver = BaotaBackupDriver::new(RefuseAll);
        let bundle = BaotaBundle::new(vec!["example.com".to_string()], vec!["mydb".to_string()]);
        let plan = driver.dry_run(&bundle).await.unwrap();
        assert_eq!(plan.resources().len(), 2);
    }

    #[tokio::test]
    async fn baota_dry_run_marks_unsupported_as_warning() {
        let driver = BaotaBackupDriver::new(RefuseAll);
        let bundle = BaotaBundle::new(vec!["example.com".to_string()], vec![]);
        let plan = driver.dry_run(&bundle).await.unwrap();
        assert!(!plan.warnings().is_empty());
    }
}
