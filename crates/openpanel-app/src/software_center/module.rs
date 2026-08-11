//! Software Center module composition and deterministic package adapter.

use std::{
    collections::BTreeSet,
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use openpanel_core::{AppContext, Migration, Module};
use openpanel_domain::software_center::{PlanAction, SupportedPlatform};
use rand::{Rng, distributions::Alphanumeric};
use sha2::{Digest, Sha256};

use super::{
    ApplicationDeployer, ApplicationDeploymentInput, AptPackageManager, HostSnapshot,
    IntegratedApplicationDeployer, OpenPanelApplicationResources, PackageManager,
    ProvisionedApplication, SoftwareCenterError, SoftwareCenterService, TokioPackageCommand,
};
use crate::{DatabasesService, SitesService};

/// Deterministic in-memory package manager for tests and development.
#[derive(Default)]
pub struct FakePackageManager {
    installed: Mutex<BTreeSet<String>>,
}
impl FakePackageManager {
    fn snapshot(&self) -> Result<HostSnapshot, SoftwareCenterError> {
        let installed = self
            .installed
            .lock()
            .map_err(|_| SoftwareCenterError::Repository)?;
        let canonical = installed.iter().cloned().collect::<Vec<_>>().join("\n");
        Ok(HostSnapshot {
            platform: SupportedPlatform::new("ubuntu", "24.04", "x86_64")
                .map_err(|_| SoftwareCenterError::Invalid)?,
            state_digest: hex::encode(Sha256::digest(canonical.as_bytes())),
            installed_packages: installed.clone(),
        })
    }
}
#[async_trait]
impl PackageManager for FakePackageManager {
    async fn discover(&self) -> Result<HostSnapshot, SoftwareCenterError> {
        self.snapshot()
    }

    async fn apply(&self, actions: &[PlanAction]) -> Result<(), SoftwareCenterError> {
        let mut installed = self
            .installed
            .lock()
            .map_err(|_| SoftwareCenterError::Repository)?;
        for action in actions {
            match action {
                PlanAction::Install(package) | PlanAction::Update(package) => {
                    installed.insert(package.as_str().to_owned());
                }
                PlanAction::Remove(package) => {
                    installed.remove(package.as_str());
                }
            }
        }
        Ok(())
    }

    async fn validate(&self, _component: &str) -> Result<(), SoftwareCenterError> {
        Ok(())
    }

    async fn rollback(&self, actions: &[PlanAction]) -> Result<(), SoftwareCenterError> {
        let mut installed = self
            .installed
            .lock()
            .map_err(|_| SoftwareCenterError::Repository)?;
        for action in actions.iter().rev() {
            if let PlanAction::Install(package) = action {
                installed.remove(package.as_str());
            }
        }
        Ok(())
    }
}

/// Deterministic-resource application adapter for tests and development.
#[derive(Default)]
pub struct FakeApplicationDeployer;
#[async_trait]
impl ApplicationDeployer for FakeApplicationDeployer {
    fn capabilities(&self) -> super::ApplicationCapabilities {
        super::ApplicationCapabilities {
            dns: true,
            tls: true,
            backups: true,
        }
    }

    async fn provision(
        &self,
        _actor: uuid::Uuid,
        input: &ApplicationDeploymentInput,
    ) -> Result<ProvisionedApplication, SoftwareCenterError> {
        let password: String = rand::thread_rng()
            .sample_iter(&Alphanumeric)
            .take(24)
            .map(char::from)
            .collect();
        Ok(ProvisionedApplication::test(
            vec![
                &format!("site:{}", input.domain),
                &format!("database:{}", input.application),
                &format!("artifact:{}", input.application),
            ],
            "openpanel-admin",
            &password,
        ))
    }

    async fn validate(
        &self,
        _deployment: &ProvisionedApplication,
    ) -> Result<(), SoftwareCenterError> {
        Ok(())
    }

    async fn rollback(
        &self,
        _deployment: &ProvisionedApplication,
    ) -> Result<(), SoftwareCenterError> {
        Ok(())
    }
}

/// Software Center bounded-context module.
pub struct SoftwareCenterModule {
    service: Arc<SoftwareCenterService>,
    migrations: Vec<Migration>,
}
impl SoftwareCenterModule {
    /// Compose the production APT adapter, or deterministic adapter when configured.
    pub async fn new(
        ctx: &AppContext,
        sites: Arc<SitesService>,
        databases: Arc<DatabasesService>,
    ) -> Result<Self, SoftwareCenterError> {
        let packages: Arc<dyn PackageManager> =
            if std::env::var("OPENPANEL__SOFTWARE__ADAPTER").as_deref() == Ok("fake") {
                Arc::new(FakePackageManager::default())
            } else {
                Arc::new(AptPackageManager::new(Arc::new(TokioPackageCommand)))
            };
        let fake = std::env::var("OPENPANEL__SOFTWARE__ADAPTER").as_deref() == Ok("fake");
        let applications: Arc<dyn ApplicationDeployer> = if fake {
            Arc::new(FakeApplicationDeployer)
        } else {
            Arc::new(IntegratedApplicationDeployer::new(Arc::new(
                OpenPanelApplicationResources::new(sites, databases)?,
            )))
        };
        Ok(Self::compose(ctx, packages, applications, true).await)
    }

    /// Compose the deterministic in-memory adapter.
    pub async fn memory(ctx: &AppContext) -> Result<Self, SoftwareCenterError> {
        Ok(Self::compose(
            ctx,
            Arc::new(FakePackageManager::default()),
            Arc::new(FakeApplicationDeployer),
            true,
        )
        .await)
    }

    async fn compose(
        ctx: &AppContext,
        packages: Arc<dyn PackageManager>,
        applications: Arc<dyn ApplicationDeployer>,
        applications_enabled: bool,
    ) -> Self {
        let pool = ctx.db.pool().await;
        let service = SoftwareCenterService::with_persistence(
            packages,
            applications,
            ctx.audit.clone(),
            pool,
            applications_enabled,
        );
        // Materialize the embedded seed on first boot so the storefront is
        // never empty before the first remote refresh succeeds.
        let embedded = crate::software_center::EmbeddedCatalogSource::new(
            crate::software_center::default_catalog_url(),
        );
        let _ = service.store().materialize_seed_if_empty(&embedded).await;
        Self {
            service: Arc::new(service),
            migrations: vec![
                Migration {
                    module: "software-center",
                    version: "001".into(),
                    description: "trusted catalogs, plans, jobs, locks, and deployments".into(),
                    sql: crate::migrations::SOFTWARE_CENTER_V001.into(),
                },
                Migration {
                    module: "software-center",
                    version: "002".into(),
                    description: "normalized catalog tables for the aggregator model".into(),
                    sql: crate::migrations::SOFTWARE_CENTER_V002.into(),
                },
                Migration {
                    module: "software-center",
                    version: "003".into(),
                    description: "add platforms_json to software_entries".into(),
                    sql: crate::migrations::SOFTWARE_CENTER_V003.into(),
                },
            ],
        }
    }

    /// Shared Software Center service.
    pub fn service(&self) -> Arc<SoftwareCenterService> {
        self.service.clone()
    }
}
impl Module for SoftwareCenterModule {
    fn name(&self) -> &'static str {
        "software-center"
    }

    fn migrations(&self) -> Vec<Migration> {
        self.migrations.clone()
    }
}
