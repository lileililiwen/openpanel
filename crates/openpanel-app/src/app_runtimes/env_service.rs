//! Per-runtime environment persistence: secrets encrypted at rest in
//! the crypto layout, audit listing changed keys only.

use std::sync::Arc;

use openpanel_core::{AuditAction, AuditEvent, AuditOutcome, AuditService};
use openpanel_domain::app_runtimes::EnvSet;
use sqlx::SqlitePool;

/// Read/write access to one runtime's environment set.
pub struct RuntimeEnvService {
    pool: SqlitePool,
    audit: Arc<dyn AuditService>,
    master_key: [u8; 32],
}

/// A stored variable as returned to surfaces: secret values are
/// never included.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct EnvVarView {
    /// Variable key.
    pub key: String,
    /// Plaintext value for non-secrets; omitted for secrets.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    /// Whether this variable rides the 0600 env file.
    pub secret: bool,
}

impl RuntimeEnvService {
    /// Construct over the shared pool with the encryption master key.
    pub fn new(pool: SqlitePool, audit: Arc<dyn AuditService>, master_key: [u8; 32]) -> Self {
        Self {
            pool,
            audit,
            master_key,
        }
    }

    async fn load_json(
        &self,
        runtime_id: uuid::Uuid,
    ) -> Result<Option<serde_json::Value>, openpanel_domain::app_runtimes::RuntimeError> {
        let payload: Option<String> =
            sqlx::query_scalar("SELECT payload FROM runtime_env WHERE runtime_id = ?")
                .bind(runtime_id.to_string())
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| {
                    openpanel_domain::app_runtimes::RuntimeError::Persistence(e.to_string())
                })?;
        Ok(payload.and_then(|json| serde_json::from_str(&json).ok()))
    }

    /// Load the environment set; unset runtimes read back as empty.
    pub async fn get(
        &self,
        caller: &openpanel_domain::User,
        runtime_id: uuid::Uuid,
    ) -> Result<Vec<EnvVarView>, openpanel_domain::app_runtimes::RuntimeError> {
        use openpanel_domain::Role;
        if caller.role() != Role::Owner && caller.role() != Role::Admin {
            return Err(openpanel_domain::app_runtimes::RuntimeError::Forbidden);
        }
        let Some(json) = self.load_json(runtime_id).await? else {
            return Ok(Vec::new());
        };
        let mut views = Vec::new();
        if let Some(vars) = json.as_array() {
            for var in vars {
                let key = var["key"].as_str().unwrap_or_default().to_string();
                let secret = var["secret"].as_bool().unwrap_or(false);
                let cipher = var["cipher"].as_str().map(str::to_string);
                let value = if secret {
                    None
                } else {
                    var["value"].as_str().map(str::to_string)
                };
                views.push(EnvVarView { key, value, secret });
                let _ = cipher;
            }
        }
        Ok(views)
    }

    /// Validate and persist the set. Secret values are stored only as
    /// AES-256-GCM ciphertext; the audit event lists changed keys.
    /// Persist the set (alias mirroring REST vocabulary).
    pub async fn set(
        &self,
        caller: &openpanel_domain::User,
        runtime_id: uuid::Uuid,
        set: EnvSet,
    ) -> Result<(), openpanel_domain::app_runtimes::RuntimeError> {
        use openpanel_domain::Role;
        if caller.role() != Role::Owner && caller.role() != Role::Admin {
            return Err(openpanel_domain::app_runtimes::RuntimeError::Forbidden);
        }
        let previous = self
            .load_json(runtime_id)
            .await?
            .unwrap_or(serde_json::Value::Null);
        let mut entries = Vec::new();
        for var in set.vars() {
            if var.secret {
                let cipher =
                    crate::databases::crypto::encrypt_to_storage(&self.master_key, &var.value)
                        .map_err(|e| {
                            openpanel_domain::app_runtimes::RuntimeError::Persistence(e.to_string())
                        })?;
                entries.push(serde_json::json!({
                    "key": var.key.as_str(),
                    "secret": true,
                    "cipher": cipher,
                }));
            } else {
                entries.push(serde_json::json!({
                    "key": var.key.as_str(),
                    "secret": false,
                    "value": var.value,
                }));
            }
        }
        let payload = serde_json::to_string(&entries).map_err(|e| {
            openpanel_domain::app_runtimes::RuntimeError::Persistence(e.to_string())
        })?;
        sqlx::query(
            "INSERT INTO runtime_env (runtime_id, payload) VALUES (?, ?) \
             ON CONFLICT(runtime_id) DO UPDATE SET payload = excluded.payload",
        )
        .bind(runtime_id.to_string())
        .bind(&payload)
        .execute(&self.pool)
        .await
        .map_err(|e| openpanel_domain::app_runtimes::RuntimeError::Persistence(e.to_string()))?;

        // Audit lists changed keys only — never values.
        let old_keys: Vec<String> = previous
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|v| v["key"].as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        let new_keys: Vec<String> = set
            .vars()
            .iter()
            .map(|v| v.key.as_str().to_string())
            .collect();
        let mut changed: Vec<String> = new_keys
            .iter()
            .filter(|k| !old_keys.contains(k))
            .cloned()
            .collect();
        changed.extend(old_keys.iter().filter(|k| !new_keys.contains(k)).cloned());
        let _ = self
            .audit
            .record(
                AuditEvent::new(
                    caller.username().as_str(),
                    AuditAction::RuntimeChanged,
                    AuditOutcome::Success,
                )
                .target(runtime_id.to_string())
                .metadata(serde_json::json!({ "changed_keys": changed })),
            )
            .await;
        Ok(())
    }

    /// Render the 0600 env-file contents from the stored set,
    /// decrypting secrets. Returns an empty string when unset.
    pub async fn render_env_file(
        &self,
        runtime_id: uuid::Uuid,
    ) -> Result<String, openpanel_domain::app_runtimes::RuntimeError> {
        let Some(json) = self.load_json(runtime_id).await? else {
            return Ok(String::new());
        };
        let mut out = String::new();
        if let Some(vars) = json.as_array() {
            for var in vars {
                let key = var["key"].as_str().unwrap_or_default();
                if var["secret"].as_bool().unwrap_or(false) {
                    let cipher = var["cipher"].as_str().unwrap_or_default();
                    let plain =
                        crate::databases::crypto::decrypt_from_storage(&self.master_key, cipher)
                            .map_err(|e| {
                                openpanel_domain::app_runtimes::RuntimeError::Persistence(
                                    e.to_string(),
                                )
                            })?;
                    out.push_str(&format!("{key}={plain}\n"));
                } else {
                    let value = var["value"].as_str().unwrap_or_default();
                    out.push_str(&format!("{key}={value}\n"));
                }
            }
        }
        Ok(out)
    }
}
