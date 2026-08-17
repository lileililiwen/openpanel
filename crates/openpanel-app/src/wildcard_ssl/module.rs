//! Wildcard SSL composition module and migration registration.

use std::sync::Arc;

use openpanel_core::{AppContext, Migration, Module};

use super::{Dns01ChallengeSolver, SqliteWildcardRepository, WildcardIssuer};

/// Stable wildcard SSL module name.
pub const MODULE_NAME: &str = "wildcard_ssl";

/// Wildcard SSL bounded-context composition root.
pub struct WildcardSslModule {
    repo: Arc<SqliteWildcardRepository>,
    solver: Arc<Dns01ChallengeSolver>,
    issuer: Arc<WildcardIssuer>,
    migrations: Vec<Migration>,
}

impl WildcardSslModule {
    /// Compose the bounded context.
    pub async fn new(ctx: &AppContext, dns: Arc<dyn super::service::DnsProviderPort>) -> Self {
        let pool = ctx.db.pool().await;
        let repo = Arc::new(SqliteWildcardRepository::new(pool));
        let solver = Arc::new(Dns01ChallengeSolver::new(
            repo.clone(),
            dns.clone(),
            ctx.audit.clone(),
        ));
        let issuer = Arc::new(WildcardIssuer::new(
            repo.clone(),
            Dns01ChallengeSolver::new(repo.clone(), dns, ctx.audit.clone()),
            ctx.audit.clone(),
        ));
        Self {
            repo,
            solver,
            issuer,
            migrations: vec![Migration {
                module: MODULE_NAME,
                version: "001".to_owned(),
                description: "wildcard SSL with DNS-01: cert requests and TXT leases".to_owned(),
                sql: crate::migrations::WILDCARD_SSL_V001.to_owned(),
            }],
        }
    }

    /// Shared issuer.
    pub fn issuer(&self) -> Arc<WildcardIssuer> {
        self.issuer.clone()
    }

    /// Shared solver.
    pub fn solver(&self) -> Arc<Dns01ChallengeSolver> {
        self.solver.clone()
    }

    /// Shared repository.
    pub fn repo(&self) -> Arc<SqliteWildcardRepository> {
        self.repo.clone()
    }
}

impl Module for WildcardSslModule {
    fn name(&self) -> &'static str {
        MODULE_NAME
    }

    fn migrations(&self) -> Vec<Migration> {
        self.migrations.clone()
    }
}
