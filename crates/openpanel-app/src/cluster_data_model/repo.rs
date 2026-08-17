//! SQLite-backed adapter for the cluster-data-model bounded context.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use openpanel_domain::{
    ClusterError, ClusterNode, ClusterRepository, FailoverPolicy, NodeId, ReplicatedDatabase,
    ReplicationMode, SharedStorage, SharedStorageKind, StorageId,
};
use sqlx::{Pool, Sqlite};
use uuid::Uuid;

use crate::cluster_data_model::service::{parse_role_json, role_to_json};

/// SQLite-backed cluster data model repository.
#[derive(Clone)]
pub struct SqliteClusterRepository {
    pool: Pool<Sqlite>,
}

impl SqliteClusterRepository {
    /// Build a repo over the given pool.
    pub fn new(pool: Pool<Sqlite>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ClusterRepository for SqliteClusterRepository {
    async fn insert_node(&self, node: &ClusterNode) -> Result<(), ClusterError> {
        let labels_json = serde_json::to_string(node.labels())
            .map_err(|e| ClusterError::Persistence(e.to_string()))?;
        let role_json = role_to_json(node.role())?;
        sqlx::query("INSERT INTO cluster_nodes (id, host_fingerprint, role_json, region, rack, labels_json, last_seen_at) VALUES (?, ?, ?, ?, ?, ?, ?)")
            .bind(node.id().as_uuid().to_string())
            .bind(node.host_fingerprint())
            .bind(role_json)
            .bind(node.region())
            .bind(node.rack())
            .bind(labels_json)
            .bind(node.last_seen_at().to_rfc3339())
            .execute(&self.pool)
            .await
            .map_err(|e| ClusterError::Persistence(e.to_string()))?;
        Ok(())
    }

    async fn find_node(&self, id: NodeId) -> Result<Option<ClusterNode>, ClusterError> {
        let row: Option<NodeRow> = sqlx::query_as::<_, NodeRow>(
            "SELECT id, host_fingerprint, role_json, region, rack, labels_json, last_seen_at FROM cluster_nodes WHERE id = ?",
        )
        .bind(id.as_uuid().to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| ClusterError::Persistence(e.to_string()))?;
        row.map(NodeRow::into_node).transpose()
    }

    async fn update_node(&self, node: &ClusterNode) -> Result<(), ClusterError> {
        let labels_json = serde_json::to_string(node.labels())
            .map_err(|e| ClusterError::Persistence(e.to_string()))?;
        let role_json = role_to_json(node.role())?;
        sqlx::query("UPDATE cluster_nodes SET role_json = ?, region = ?, rack = ?, labels_json = ?, last_seen_at = ? WHERE id = ?")
            .bind(role_json)
            .bind(node.region())
            .bind(node.rack())
            .bind(labels_json)
            .bind(node.last_seen_at().to_rfc3339())
            .bind(node.id().as_uuid().to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| ClusterError::Persistence(e.to_string()))?;
        Ok(())
    }

    async fn list_nodes(&self) -> Result<Vec<ClusterNode>, ClusterError> {
        let rows: Vec<NodeRow> = sqlx::query_as::<_, NodeRow>(
            "SELECT id, host_fingerprint, role_json, region, rack, labels_json, last_seen_at FROM cluster_nodes ORDER BY last_seen_at",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| ClusterError::Persistence(e.to_string()))?;
        rows.into_iter().map(NodeRow::into_node).collect()
    }

    async fn insert_storage(&self, storage: &SharedStorage) -> Result<(), ClusterError> {
        let sites_json = serde_json::to_string(storage.sites_attached())
            .map_err(|e| ClusterError::Persistence(e.to_string()))?;
        let backups_json = serde_json::to_string(storage.backups_attached())
            .map_err(|e| ClusterError::Persistence(e.to_string()))?;
        let _ = StorageId; // suppress unused
        let _ = sites_json;
        let _ = backups_json;
        sqlx::query("INSERT INTO shared_storage (id, kind, mount_path, sites_attached_json, backups_attached_json) VALUES (?, ?, ?, ?, ?)")
            .bind(storage.id().as_uuid().to_string())
            .bind(kind_str(storage.kind()))
            .bind(storage_to_path_string(storage.mount_path()))
            .bind(serde_json::to_string(storage.sites_attached())
                .map_err(|e| ClusterError::Persistence(e.to_string()))?)
            .bind(serde_json::to_string(storage.backups_attached())
                .map_err(|e| ClusterError::Persistence(e.to_string()))?)
            .execute(&self.pool)
            .await
            .map_err(|e| ClusterError::Persistence(e.to_string()))?;
        Ok(())
    }

    async fn find_storage(&self, id: StorageId) -> Result<Option<SharedStorage>, ClusterError> {
        let row: Option<StorageRow> = sqlx::query_as::<_, StorageRow>(
            "SELECT id, kind, mount_path, sites_attached_json, backups_attached_json FROM shared_storage WHERE id = ?",
        )
        .bind(id.as_uuid().to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| ClusterError::Persistence(e.to_string()))?;
        row.map(StorageRow::into_storage).transpose()
    }

    async fn list_storage(&self) -> Result<Vec<SharedStorage>, ClusterError> {
        let rows: Vec<StorageRow> = sqlx::query_as::<_, StorageRow>(
            "SELECT id, kind, mount_path, sites_attached_json, backups_attached_json FROM shared_storage ORDER BY mount_path",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| ClusterError::Persistence(e.to_string()))?;
        rows.into_iter().map(StorageRow::into_storage).collect()
    }

    async fn update_storage(&self, storage: &SharedStorage) -> Result<(), ClusterError> {
        sqlx::query("UPDATE shared_storage SET sites_attached_json = ?, backups_attached_json = ? WHERE id = ?")
            .bind(serde_json::to_string(storage.sites_attached())
                .map_err(|e| ClusterError::Persistence(e.to_string()))?)
            .bind(serde_json::to_string(storage.backups_attached())
                .map_err(|e| ClusterError::Persistence(e.to_string()))?)
            .bind(storage.id().as_uuid().to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| ClusterError::Persistence(e.to_string()))?;
        Ok(())
    }

    async fn insert_replicated(&self, db: &ReplicatedDatabase) -> Result<(), ClusterError> {
        let replicas: Vec<String> = db
            .replicas()
            .iter()
            .map(|n| n.as_uuid().to_string())
            .collect();
        let replicas_json = serde_json::to_string(&replicas)
            .map_err(|e| ClusterError::Persistence(e.to_string()))?;
        sqlx::query("INSERT INTO replicated_databases (database_id, primary_node_id, replicas_json, replication_mode, failover_policy) VALUES (?, ?, ?, ?, ?)")
            .bind(db.database_id().to_string())
            .bind(db.primary_node_id().as_uuid().to_string())
            .bind(replicas_json)
            .bind(replication_mode_str(db.replication_mode()))
            .bind(failover_policy_str(db.failover_policy()))
            .execute(&self.pool)
            .await
            .map_err(|e| ClusterError::Persistence(e.to_string()))?;
        Ok(())
    }

    async fn find_replicated(
        &self,
        database_id: Uuid,
    ) -> Result<Option<ReplicatedDatabase>, ClusterError> {
        let row: Option<ReplicatedRow> = sqlx::query_as::<_, ReplicatedRow>(
            "SELECT database_id, primary_node_id, replicas_json, replication_mode, failover_policy FROM replicated_databases WHERE database_id = ?",
        )
        .bind(database_id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| ClusterError::Persistence(e.to_string()))?;
        row.map(ReplicatedRow::into_replicated).transpose()
    }

    async fn list_replicated(&self) -> Result<Vec<ReplicatedDatabase>, ClusterError> {
        let rows: Vec<ReplicatedRow> = sqlx::query_as::<_, ReplicatedRow>(
            "SELECT database_id, primary_node_id, replicas_json, replication_mode, failover_policy FROM replicated_databases",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| ClusterError::Persistence(e.to_string()))?;
        rows.into_iter()
            .map(ReplicatedRow::into_replicated)
            .collect()
    }
}

fn kind_str(kind: SharedStorageKind) -> &'static str {
    match kind {
        SharedStorageKind::Nfs => "nfs",
        SharedStorageKind::Cephfs => "cephfs",
        SharedStorageKind::Glusterfs => "glusterfs",
    }
}

fn storage_to_path_string(p: &std::path::Path) -> String {
    p.to_string_lossy().into_owned()
}

fn replication_mode_str(mode: ReplicationMode) -> &'static str {
    match mode {
        ReplicationMode::Async => "async",
        ReplicationMode::SemiSync => "semi_sync",
        ReplicationMode::Sync => "sync",
    }
}

fn parse_replication_mode(s: &str) -> Result<ReplicationMode, ClusterError> {
    match s {
        "async" => Ok(ReplicationMode::Async),
        "semi_sync" => Ok(ReplicationMode::SemiSync),
        "sync" => Ok(ReplicationMode::Sync),
        other => Err(ClusterError::Persistence(format!(
            "unknown replication mode `{other}`"
        ))),
    }
}

fn failover_policy_str(p: FailoverPolicy) -> &'static str {
    match p {
        FailoverPolicy::Auto => "auto",
        FailoverPolicy::Manual => "manual",
    }
}

fn parse_failover_policy(s: &str) -> Result<FailoverPolicy, ClusterError> {
    match s {
        "auto" => Ok(FailoverPolicy::Auto),
        "manual" => Ok(FailoverPolicy::Manual),
        other => Err(ClusterError::Persistence(format!(
            "unknown failover policy `{other}`"
        ))),
    }
}

fn parse_storage_kind(s: &str) -> Result<SharedStorageKind, ClusterError> {
    match s {
        "nfs" => Ok(SharedStorageKind::Nfs),
        "cephfs" => Ok(SharedStorageKind::Cephfs),
        "glusterfs" => Ok(SharedStorageKind::Glusterfs),
        other => Err(ClusterError::Persistence(format!(
            "unknown storage kind `{other}`"
        ))),
    }
}

#[derive(sqlx::FromRow)]
struct NodeRow {
    id: String,
    host_fingerprint: String,
    role_json: String,
    region: String,
    rack: String,
    labels_json: String,
    last_seen_at: String,
}

impl NodeRow {
    fn into_node(self) -> Result<ClusterNode, ClusterError> {
        let id = Uuid::parse_str(&self.id)
            .map_err(|e| ClusterError::Persistence(format!("bad id: {e}")))?;
        let role = parse_role_json(&self.role_json)?;
        let labels: std::collections::BTreeMap<String, String> =
            serde_json::from_str(&self.labels_json)
                .map_err(|e| ClusterError::Persistence(e.to_string()))?;
        let last_seen_at = parse_dt(&self.last_seen_at)?;
        Ok(ClusterNode::restore(
            NodeId(id),
            self.host_fingerprint,
            role,
            self.region,
            self.rack,
            labels,
            last_seen_at,
        ))
    }
}

#[derive(sqlx::FromRow)]
struct StorageRow {
    id: String,
    kind: String,
    mount_path: String,
    sites_attached_json: String,
    backups_attached_json: String,
}

impl StorageRow {
    fn into_storage(self) -> Result<SharedStorage, ClusterError> {
        let id = Uuid::parse_str(&self.id)
            .map_err(|e| ClusterError::Persistence(format!("bad id: {e}")))?;
        let kind = parse_storage_kind(&self.kind)?;
        let sites_attached: Vec<Uuid> = serde_json::from_str(&self.sites_attached_json)
            .map_err(|e| ClusterError::Persistence(e.to_string()))?;
        let backups_attached: Vec<Uuid> = serde_json::from_str(&self.backups_attached_json)
            .map_err(|e| ClusterError::Persistence(e.to_string()))?;
        Ok(SharedStorage::restore(
            StorageId(id),
            kind,
            std::path::PathBuf::from(self.mount_path),
            sites_attached,
            backups_attached,
        ))
    }
}

#[derive(sqlx::FromRow)]
struct ReplicatedRow {
    database_id: String,
    primary_node_id: String,
    replicas_json: String,
    replication_mode: String,
    failover_policy: String,
}

impl ReplicatedRow {
    fn into_replicated(self) -> Result<ReplicatedDatabase, ClusterError> {
        let database_id = Uuid::parse_str(&self.database_id)
            .map_err(|e| ClusterError::Persistence(format!("bad database_id: {e}")))?;
        let primary_node_id = Uuid::parse_str(&self.primary_node_id)
            .map_err(|e| ClusterError::Persistence(format!("bad primary_node_id: {e}")))?;
        let replica_ids: Vec<Uuid> = serde_json::from_str(&self.replicas_json)
            .map_err(|e| ClusterError::Persistence(e.to_string()))?;
        let replicas = replica_ids.into_iter().map(NodeId).collect::<Vec<_>>();
        let replication_mode = parse_replication_mode(&self.replication_mode)?;
        let failover_policy = parse_failover_policy(&self.failover_policy)?;
        Ok(ReplicatedDatabase::restore(
            database_id,
            NodeId(primary_node_id),
            replicas,
            replication_mode,
            failover_policy,
        ))
    }
}

fn parse_dt(s: &str) -> Result<DateTime<Utc>, ClusterError> {
    DateTime::parse_from_rfc3339(s)
        .map(|d| d.with_timezone(&Utc))
        .map_err(|e| ClusterError::Persistence(format!("bad timestamp `{s}`: {e}")))
}
