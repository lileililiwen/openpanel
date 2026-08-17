//! Cluster data model application layer: minimal persistence +
//! service that exposes the topology, shared storage, and
//! replicated database CRUD paths the API layer consumes.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use openpanel_core::{AuditAction, AuditEvent, AuditOutcome, AuditService};
use openpanel_domain::{
    ClusterError, ClusterNode, ClusterRepository, ClusterTopology, FailoverPolicy, NodeId,
    NodeRole, ReplicatedDatabase, ReplicationMode, SharedStorage, SharedStorageKind, StorageId,
};
use serde::{Deserialize, Serialize};
use sqlx::{Pool, Sqlite};
use uuid::Uuid;

use crate::cluster_data_model::repo::SqliteClusterRepository;

const NODE_ROLE_PRIMARY: &str = "primary";
const NODE_ROLE_EDGE: &str = "edge";
const NODE_ROLE_REPLICA: &str = "replica";

fn role_to_str(role: &NodeRole) -> &'static str {
    match role {
        NodeRole::Primary => NODE_ROLE_PRIMARY,
        NodeRole::Edge => NODE_ROLE_EDGE,
        NodeRole::Replica { .. } => NODE_ROLE_REPLICA,
    }
}

/// Persistent form of a `NodeRole` (replica carries the primary).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum NodeRoleRow {
    Primary,
    Replica { primary_node_id: Uuid },
    Edge,
}

impl From<&NodeRole> for NodeRoleRow {
    fn from(role: &NodeRole) -> Self {
        match role {
            NodeRole::Primary => NodeRoleRow::Primary,
            NodeRole::Edge => NodeRoleRow::Edge,
            NodeRole::Replica { primary_node_id } => NodeRoleRow::Replica {
                primary_node_id: primary_node_id.0,
            },
        }
    }
}

impl From<NodeRoleRow> for NodeRole {
    fn from(row: NodeRoleRow) -> Self {
        match row {
            NodeRoleRow::Primary => NodeRole::Primary,
            NodeRoleRow::Edge => NodeRole::Edge,
            NodeRoleRow::Replica { primary_node_id } => NodeRole::Replica {
                primary_node_id: NodeId(primary_node_id),
            },
        }
    }
}

/// Cluster data model application service.
#[derive(Clone)]
pub struct ClusterService {
    repo: Arc<dyn ClusterRepository>,
    audit: Arc<dyn AuditService>,
}

impl ClusterService {
    /// Build a service over the repository.
    pub fn new(repo: Arc<dyn ClusterRepository>, audit: Arc<dyn AuditService>) -> Self {
        Self { repo, audit }
    }

    /// Build a service backed by the SQLite adapter.
    pub fn with_sqlite(
        pool: Pool<Sqlite>,
        audit: Arc<dyn AuditService>,
    ) -> Self {
        Self::new(Arc::new(SqliteClusterRepository::new(pool)), audit)
    }

    /// Declare a new node.
    pub async fn declare_node(
        &self,
        host_fingerprint: &str,
        role: NodeRole,
        region: &str,
        rack: &str,
        actor: &str,
    ) -> Result<ClusterNode, ClusterError> {
        let node = ClusterNode::new(
            NodeId::new(),
            host_fingerprint.to_string(),
            role,
            region.to_string(),
            rack.to_string(),
            Utc::now(),
        );
        self.repo.insert_node(&node).await?;
        self.audit
            .record(
                AuditEvent::new(actor, AuditAction::ClusterNodeDeclared, AuditOutcome::Success)
                    .target(node.id().as_uuid().to_string())
                    .metadata(serde_json::json!({
                        "role": role_to_str(node.role()),
                        "region": node.region(),
                    })),
            )
            .await
            .ok();
        Ok(node)
    }

    /// Update the role of an existing node.
    pub async fn set_role(
        &self,
        id: NodeId,
        role: NodeRole,
        actor: &str,
    ) -> Result<ClusterNode, ClusterError> {
        let mut node = self
            .repo
            .find_node(id)
            .await?
            .ok_or(ClusterError::NodeNotFound)?;
        let role_for_audit = match &role {
            NodeRole::Primary => NODE_ROLE_PRIMARY,
            NodeRole::Edge => NODE_ROLE_EDGE,
            NodeRole::Replica { .. } => NODE_ROLE_REPLICA,
        };
        node.set_role(role);
        self.repo.update_node(&node).await?;
        let topology = self.topology().await?;
        if topology.has_cycle() {
            return Err(ClusterError::TopologyCycle);
        }
        self.audit
            .record(
                AuditEvent::new(actor, AuditAction::ClusterNodeRoleChanged, AuditOutcome::Success)
                    .target(id.as_uuid().to_string())
                    .metadata(serde_json::json!({"role": role_for_audit})),
            )
            .await
            .ok();
        Ok(node)
    }

    /// Return the topology.
    pub async fn topology(&self) -> Result<ClusterTopology, ClusterError> {
        let nodes = self.repo.list_nodes().await?;
        Ok(ClusterTopology::new(nodes))
    }

    /// Declare a shared storage volume.
    pub async fn declare_storage(
        &self,
        kind: SharedStorageKind,
        mount_path: std::path::PathBuf,
        actor: &str,
    ) -> Result<SharedStorage, ClusterError> {
        let storage = SharedStorage::new(StorageId::new(), kind, mount_path)?;
        self.repo.insert_storage(&storage).await?;
        self.audit
            .record(
                AuditEvent::new(actor, AuditAction::ClusterStorageDeclared, AuditOutcome::Success)
                    .target(storage.id().as_uuid().to_string())
                    .metadata(serde_json::json!({
                        "kind": storage.kind(),
                    })),
            )
            .await
            .ok();
        Ok(storage)
    }

    /// Declare a replicated database.
    pub async fn declare_replicated(
        &self,
        database_id: Uuid,
        primary_node_id: NodeId,
        replicas: Vec<NodeId>,
        replication_mode: ReplicationMode,
        failover_policy: FailoverPolicy,
        actor: &str,
    ) -> Result<ReplicatedDatabase, ClusterError> {
        let cfg = ReplicatedDatabase::new(
            database_id,
            primary_node_id,
            replicas,
            replication_mode,
            failover_policy,
        )?;
        self.repo.insert_replicated(&cfg).await?;
        self.audit
            .record(
                AuditEvent::new(
                    actor,
                    AuditAction::ClusterReplicatedDatabaseDeclared,
                    AuditOutcome::Success,
                )
                .target(database_id.to_string())
                .metadata(serde_json::json!({
                    "primary": primary_node_id.as_uuid().to_string(),
                })),
            )
            .await
            .ok();
        Ok(cfg)
    }

    /// List replicated databases.
    pub async fn list_replicated(&self) -> Result<Vec<ReplicatedDatabase>, ClusterError> {
        self.repo.list_replicated().await
    }
}

/// Serialization helper for `NodeRoleRow` (`replica` carries the
/// primary node id).
pub fn parse_role_json(json: &str) -> Result<NodeRole, ClusterError> {
    let row: NodeRoleRow = serde_json::from_str(json)
        .map_err(|e| ClusterError::Persistence(format!("bad role json: {e}")))?;
    Ok(row.into())
}

/// Convert a role to its JSON form.
pub fn role_to_json(role: &NodeRole) -> Result<String, ClusterError> {
    serde_json::to_string(&NodeRoleRow::from(role))
        .map_err(|e| ClusterError::Persistence(format!("bad role json: {e}")))
}

/// Default for [`Default`]: re-export the role tag.
pub fn role_tag(role: &NodeRole) -> &'static str {
    role_to_str(role)
}

#[allow(dead_code)]
fn _datetime_marker() -> DateTime<Utc> {
    Utc::now()
}
