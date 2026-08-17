//! Cluster data model module: composition root.

use std::sync::Arc;

use openpanel_core::{AppContext, Migration, Module};

use crate::cluster_data_model::ClusterService;

/// Stable module name.
pub const MODULE_NAME: &str = "cluster-data-model";

/// Cluster data model module.
pub struct ClusterDataModelModule {
    service: Arc<ClusterService>,
    migrations: Vec<Migration>,
}

impl ClusterDataModelModule {
    /// Build the module from the panel application context.
    pub async fn new(ctx: &AppContext) -> Self {
        let pool = ctx.db.pool().await;
        let service = Arc::new(ClusterService::with_sqlite(pool, ctx.audit.clone()));
        Self {
            service,
            migrations: vec![Migration {
                module: MODULE_NAME,
                version: "001".to_string(),
                description: "cluster nodes, shared storage, replicated databases".to_string(),
                sql: crate::migrations::CLUSTER_DATA_MODEL_V001.to_string(),
            }],
        }
    }

    /// Return the shared service handle.
    pub fn service(&self) -> Arc<ClusterService> {
        self.service.clone()
    }
}

impl Module for ClusterDataModelModule {
    fn name(&self) -> &'static str {
        MODULE_NAME
    }

    fn migrations(&self) -> Vec<Migration> {
        self.migrations.clone()
    }

    fn config_schema(&self) -> serde_json::Value {
        serde_json::json!({"type": "object", "properties": {}})
    }
}
