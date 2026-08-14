//! `collaborators` module composition.

use std::sync::Arc;

use openpanel_core::{AppContext, AuditService, Migration, Module};

use super::{
    repo::{SqliteCollaboratorRepository, SqliteSiteGrantRepository},
    service::{CollaboratorService, GrantResolver},
};

/// Stable module identifier.
pub const MODULE_NAME: &str = "collaborators";

/// Per-site collaborator module.
pub struct CollaboratorsModule {
    service: Arc<CollaboratorService>,
    resolver: Arc<GrantResolver>,
    migrations: Vec<Migration>,
}

impl CollaboratorsModule {
    /// Build the module from the shared `AppContext`.
    pub async fn new(ctx: &AppContext, audit: Arc<dyn AuditService>) -> Self {
        let pool = ctx.db.pool().await;
        let collaborators = Arc::new(SqliteCollaboratorRepository::new(pool.clone()));
        let grants = Arc::new(SqliteSiteGrantRepository::new(pool));
        let resolver = Arc::new(GrantResolver::new(grants.clone()));
        let service = Arc::new(CollaboratorService::new(
            collaborators,
            grants,
            audit,
        ));
        Self {
            service,
            resolver,
            migrations: vec![Migration {
                module: MODULE_NAME.into(),
                version: "001".into(),
                description: "collaborators initial schema".into(),
                sql: crate::migrations::COLLABORATORS_V001.into(),
            }],
        }
    }

    /// Access the collaborator service.
    pub fn service(&self) -> Arc<CollaboratorService> {
        self.service.clone()
    }

    /// Access the grant resolver.
    pub fn resolver(&self) -> Arc<GrantResolver> {
        self.resolver.clone()
    }
}

impl Module for CollaboratorsModule {
    fn name(&self) -> &'static str {
        MODULE_NAME
    }

    fn migrations(&self) -> Vec<Migration> {
        self.migrations.clone()
    }
}