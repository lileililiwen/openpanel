//! API-token bounded-context composition root.

use std::sync::Arc;

use openpanel_core::{AppContext, Migration, Module};
use openpanel_domain::{ApiTokenRepository, UserRepository};

use super::{ApiTokenService, RateLimitConfig, SqliteApiTokenRepository};
use crate::identity::repo::SqliteUserRepository;

/// Stable module name.
pub const MODULE_NAME: &str = "api-tokens";

/// API-token module.
pub struct ApiTokenModule {
    service: Arc<ApiTokenService>,
    migrations: Vec<Migration>,
}

impl ApiTokenModule {
    /// Compose repositories and token services under the panel master-key pepper.
    pub async fn new(ctx: &AppContext, pepper: [u8; 32]) -> Self {
        let pool = ctx.db.pool().await;
        let repo: Arc<dyn ApiTokenRepository> =
            Arc::new(SqliteApiTokenRepository::new(pool.clone()));
        let users: Arc<dyn UserRepository> = Arc::new(SqliteUserRepository::new(pool));
        let mut rate: RateLimitConfig = ctx
            .config
            .modules
            .get(MODULE_NAME)
            .and_then(|value| serde_json::from_value(value.clone()).ok())
            .unwrap_or_default();
        if let Some(burst) = positive_env_u32("OPENPANEL__API__RATE_BURST") {
            rate.burst = burst;
        }
        if let Some(per_minute) = positive_env_u32("OPENPANEL__API__RATE_PER_MIN") {
            rate.per_minute = per_minute;
        }
        Self {
            service: Arc::new(ApiTokenService::new(
                repo,
                users,
                ctx.audit.clone(),
                pepper,
                rate,
            )),
            migrations: vec![Migration {
                module: MODULE_NAME,
                version: "001".into(),
                description: "scoped personal API tokens".into(),
                sql: crate::migrations::API_TOKENS_V001.into(),
            }],
        }
    }

    /// Shared lifecycle and bearer-authentication service.
    pub fn service(&self) -> Arc<ApiTokenService> {
        self.service.clone()
    }
}

fn positive_env_u32(name: &str) -> Option<u32> {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|value| *value > 0)
}

impl Module for ApiTokenModule {
    fn name(&self) -> &'static str {
        MODULE_NAME
    }

    fn migrations(&self) -> Vec<Migration> {
        self.migrations.clone()
    }

    fn config_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "burst": { "type": "integer", "minimum": 1 },
                "per_minute": { "type": "integer", "minimum": 1 }
            }
        })
    }
}
