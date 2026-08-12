//! Identity module: registers the identity service, HTTP routes, CLI
//! commands, and migrations.

use std::sync::Arc;

use openpanel_core::{AppContext, Migration, Module};
use serde::Deserialize;

use crate::identity::{
    factor_repo::SqliteFactorRepository,
    service::IdentityService,
    two_factor::{TwoFactorCrypto, TwoFactorService, TwoFactorSettings},
};

/// Stable identifier for the identity module used in migration bookkeeping.
pub const MODULE_NAME: &str = "identity";

#[derive(Debug, Deserialize)]
struct IdentityConfig {
    #[serde(default)]
    webauthn_rp_id: Option<String>,
    #[serde(default)]
    webauthn_origins: Vec<String>,
    #[serde(default = "default_issuer_label")]
    issuer_label: String,
    #[serde(default = "default_challenge_lifetime_seconds")]
    challenge_lifetime_seconds: i64,
    #[serde(default = "default_recovery_code_count")]
    recovery_code_count: u8,
    #[serde(default = "default_remember_device_lifetime_days")]
    remember_device_lifetime_days: i64,
}

fn default_issuer_label() -> String {
    "OpenPanel".to_string()
}

const fn default_challenge_lifetime_seconds() -> i64 {
    300
}

const fn default_recovery_code_count() -> u8 {
    10
}

const fn default_remember_device_lifetime_days() -> i64 {
    30
}

impl Default for IdentityConfig {
    fn default() -> Self {
        Self {
            webauthn_rp_id: None,
            webauthn_origins: Vec::new(),
            issuer_label: default_issuer_label(),
            challenge_lifetime_seconds: default_challenge_lifetime_seconds(),
            recovery_code_count: default_recovery_code_count(),
            remember_device_lifetime_days: default_remember_device_lifetime_days(),
        }
    }
}

impl IdentityConfig {
    fn two_factor_settings(&self) -> TwoFactorSettings {
        TwoFactorSettings {
            issuer_label: self.issuer_label.clone(),
            challenge_lifetime: chrono::Duration::seconds(self.challenge_lifetime_seconds),
            recovery_code_count: self.recovery_code_count,
            remember_device_lifetime: chrono::Duration::days(self.remember_device_lifetime_days),
        }
    }
}

/// Identity bounded-context module: wires the service + SQLite repos + migrations.
pub struct IdentityModule {
    service: Arc<IdentityService>,
    two_factor: Arc<TwoFactorService>,
    migrations: Vec<Migration>,
}

impl IdentityModule {
    /// Build the module from an `AppContext`. The caller must provide
    /// the panel master key (used to encrypt TOTP secrets at rest).
    pub async fn new(ctx: &AppContext, master_key: [u8; 32]) -> Self {
        Self::build(ctx, master_key, None).await
    }

    /// Build with deterministic two-factor cryptography supplied by a
    /// test or alternate adapter.
    pub async fn new_with_two_factor_crypto(
        ctx: &AppContext,
        master_key: [u8; 32],
        crypto: Arc<dyn TwoFactorCrypto>,
    ) -> Self {
        Self::build(ctx, master_key, Some(crypto)).await
    }

    async fn build(
        ctx: &AppContext,
        master_key: [u8; 32],
        crypto: Option<Arc<dyn TwoFactorCrypto>>,
    ) -> Self {
        let pool = ctx.db.pool().await;
        let (users, sessions) = crate::identity::service::build_repos(pool.clone());
        let factors = Arc::new(SqliteFactorRepository::new(pool));
        let identity_config = ctx
            .config
            .modules
            .get(MODULE_NAME)
            .cloned()
            .and_then(|value| serde_json::from_value::<IdentityConfig>(value).ok())
            .unwrap_or_default();
        let two_factor = if let Some(crypto) = crypto {
            TwoFactorService::new_with_crypto(
                factors.clone(),
                master_key,
                ctx.audit.clone(),
                crypto,
            )
        } else {
            match (
                identity_config.webauthn_rp_id.as_deref(),
                identity_config.webauthn_origins.as_slice(),
            ) {
                (Some(rp_id), origins) if !origins.is_empty() => {
                    TwoFactorService::new_with_webauthn_origins(
                        factors.clone(),
                        master_key,
                        ctx.audit.clone(),
                        rp_id,
                        origins,
                    )
                    .unwrap_or_else(|error| {
                        tracing::warn!(%error, "invalid identity WebAuthn configuration; WebAuthn disabled");
                        TwoFactorService::new(factors, master_key, ctx.audit.clone())
                    })
                }
                _ => TwoFactorService::new(factors, master_key, ctx.audit.clone()),
            }
        }
        .with_settings(identity_config.two_factor_settings());
        let two_factor = Arc::new(two_factor);
        let service = Arc::new(IdentityService::new(
            users,
            sessions,
            ctx.audit.clone(),
            two_factor.clone(),
        ));
        let migrations = vec![
            Migration {
                module: MODULE_NAME,
                version: "001".to_string(),
                description: "identity initial schema".to_string(),
                sql: crate::migrations::IDENTITY_V001.to_string(),
            },
            Migration {
                module: MODULE_NAME,
                version: "002".to_string(),
                description: "two-factor authentication (TOTP, recovery codes, login challenges)"
                    .to_string(),
                sql: crate::migrations::IDENTITY_V002.to_string(),
            },
            Migration {
                module: MODULE_NAME,
                version: "003".to_string(),
                description: "WebAuthn credentials and ceremony challenges".to_string(),
                sql: crate::migrations::IDENTITY_V003.to_string(),
            },
            Migration {
                module: MODULE_NAME,
                version: "004".to_string(),
                description: "WebAuthn passkey_json column (full Passkey serialization)"
                    .to_string(),
                sql: crate::migrations::IDENTITY_V004.to_string(),
            },
        ];
        Self {
            service,
            two_factor,
            migrations,
        }
    }

    /// Return a clone of the shared identity service handle.
    pub fn service(&self) -> Arc<IdentityService> {
        self.service.clone()
    }

    /// Return a clone of the shared two-factor service handle.
    pub fn two_factor(&self) -> Arc<TwoFactorService> {
        self.two_factor.clone()
    }
}

impl Module for IdentityModule {
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
                "webauthn_rp_id": { "type": "string", "minLength": 1 },
                "webauthn_origins": {
                    "type": "array",
                    "items": { "type": "string", "format": "uri" },
                    "minItems": 1
                },
                "issuer_label": { "type": "string", "minLength": 1 },
                "challenge_lifetime_seconds": { "type": "integer", "minimum": 1 },
                "recovery_code_count": { "type": "integer", "minimum": 1, "maximum": 255 },
                "remember_device_lifetime_days": { "type": "integer", "minimum": 1 }
            }
        })
    }
}
