//! Cluster data model bounded context: typed host roles, shared
//! storage declarations, and replicated database metadata.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::RepoError;

/// Stable identifier for a cluster node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct NodeId(pub Uuid);

impl NodeId {
    /// Brand a uuid as a NodeId.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
    /// Underlying UUID.
    pub fn as_uuid(&self) -> Uuid {
        self.0
    }
}

impl Default for NodeId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for NodeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Stable identifier for a shared storage volume.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct StorageId(pub Uuid);

impl StorageId {
    /// Brand a uuid as a StorageId.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
    /// Underlying UUID.
    pub fn as_uuid(&self) -> Uuid {
        self.0
    }
}

impl Default for StorageId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for StorageId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Role of a node in the cluster.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NodeRole {
    /// The primary node accepts writes and can serve as the
    /// source of truth for replicated databases.
    Primary,
    /// A replica follows a primary.
    Replica {
        /// The primary node id.
        primary_node_id: NodeId,
    },
    /// An edge node carries sites but does not host databases.
    Edge,
}

/// A cluster node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClusterNode {
    id: NodeId,
    host_fingerprint: String,
    role: NodeRole,
    region: String,
    rack: String,
    labels: BTreeMap<String, String>,
    last_seen_at: DateTime<Utc>,
}

impl ClusterNode {
    /// Build a new cluster node.
    pub fn new(
        id: NodeId,
        host_fingerprint: impl Into<String>,
        role: NodeRole,
        region: impl Into<String>,
        rack: impl Into<String>,
        now: DateTime<Utc>,
    ) -> Self {
        Self {
            id,
            host_fingerprint: host_fingerprint.into(),
            role,
            region: region.into(),
            rack: rack.into(),
            labels: BTreeMap::new(),
            last_seen_at: now,
        }
    }
    /// Restore from persistence.
    pub fn restore(
        id: NodeId,
        host_fingerprint: String,
        role: NodeRole,
        region: String,
        rack: String,
        labels: BTreeMap<String, String>,
        last_seen_at: DateTime<Utc>,
    ) -> Self {
        Self {
            id,
            host_fingerprint,
            role,
            region,
            rack,
            labels,
            last_seen_at,
        }
    }

    /// Node id.
    pub fn id(&self) -> NodeId {
        self.id
    }
    /// Host fingerprint.
    pub fn host_fingerprint(&self) -> &str {
        &self.host_fingerprint
    }
    /// Role.
    pub fn role(&self) -> &NodeRole {
        &self.role
    }
    /// Region.
    pub fn region(&self) -> &str {
        &self.region
    }
    /// Rack.
    pub fn rack(&self) -> &str {
        &self.rack
    }
    /// Labels.
    pub fn labels(&self) -> &BTreeMap<String, String> {
        &self.labels
    }
    /// Last seen.
    pub fn last_seen_at(&self) -> DateTime<Utc> {
        self.last_seen_at
    }
    /// Set the role.
    pub fn set_role(&mut self, role: NodeRole) {
        self.role = role;
    }
    /// Touch last_seen_at.
    pub fn touch(&mut self, now: DateTime<Utc>) {
        self.last_seen_at = now;
    }
    /// Whether the node is a primary.
    pub fn is_primary(&self) -> bool {
        matches!(self.role, NodeRole::Primary)
    }
    /// Whether the node is a replica.
    pub fn is_replica(&self) -> bool {
        matches!(self.role, NodeRole::Replica { .. })
    }
    /// The primary node id, if the node is a replica.
    pub fn primary_for(&self) -> Option<NodeId> {
        match self.role {
            NodeRole::Replica { primary_node_id } => Some(primary_node_id),
            _ => None,
        }
    }
}

/// Topology graph.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClusterTopology {
    nodes: Vec<ClusterNode>,
}

impl ClusterTopology {
    /// Build from a list of nodes.
    pub fn new(nodes: Vec<ClusterNode>) -> Self {
        Self { nodes }
    }
    /// Nodes.
    pub fn nodes(&self) -> &[ClusterNode] {
        &self.nodes
    }
    /// Find a node by id.
    pub fn find(&self, id: NodeId) -> Option<&ClusterNode> {
        self.nodes.iter().find(|n| n.id() == id)
    }
    /// Detect a cycle in the replica graph. The graph is a set
    /// of edges `replica -> primary`. A cycle exists if any
    /// primary is reachable from itself via the replica edges.
    pub fn has_cycle(&self) -> bool {
        let mut by_id: BTreeMap<NodeId, &ClusterNode> = BTreeMap::new();
        for node in &self.nodes {
            by_id.insert(node.id(), node);
        }
        for node in &self.nodes {
            let mut visited: BTreeSet<NodeId> = BTreeSet::new();
            let mut current = node.id();
            while let Some(n) = by_id.get(&current) {
                let Some(primary) = n.primary_for() else {
                    break;
                };
                if !visited.insert(primary) {
                    return true;
                }
                if primary == node.id() {
                    return true;
                }
                current = primary;
            }
        }
        false
    }
}

/// Replication mode for a database.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReplicationMode {
    /// Asynchronous replication.
    Async,
    /// Semi-synchronous replication.
    SemiSync,
    /// Synchronous replication.
    Sync,
}

/// Failover policy for a replicated database.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailoverPolicy {
    /// The panel may fail over automatically.
    Auto,
    /// A human must approve failover.
    Manual,
}

/// A replicated database configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplicatedDatabase {
    database_id: Uuid,
    primary_node_id: NodeId,
    replicas: Vec<NodeId>,
    replication_mode: ReplicationMode,
    failover_policy: FailoverPolicy,
}

impl ReplicatedDatabase {
    /// Build a new replicated database config.
    pub fn new(
        database_id: Uuid,
        primary_node_id: NodeId,
        replicas: Vec<NodeId>,
        replication_mode: ReplicationMode,
        failover_policy: FailoverPolicy,
    ) -> Result<Self, ClusterError> {
        if replicas.contains(&primary_node_id) {
            return Err(ClusterError::ReplicaIsPrimary);
        }
        Ok(Self {
            database_id,
            primary_node_id,
            replicas,
            replication_mode,
            failover_policy,
        })
    }
    /// Restore from persistence.
    pub fn restore(
        database_id: Uuid,
        primary_node_id: NodeId,
        replicas: Vec<NodeId>,
        replication_mode: ReplicationMode,
        failover_policy: FailoverPolicy,
    ) -> Self {
        Self {
            database_id,
            primary_node_id,
            replicas,
            replication_mode,
            failover_policy,
        }
    }

    /// Database id.
    pub fn database_id(&self) -> Uuid {
        self.database_id
    }
    /// Primary node id.
    pub fn primary_node_id(&self) -> NodeId {
        self.primary_node_id
    }
    /// Replica node ids.
    pub fn replicas(&self) -> &[NodeId] {
        &self.replicas
    }
    /// Replication mode.
    pub fn replication_mode(&self) -> ReplicationMode {
        self.replication_mode
    }
    /// Failover policy.
    pub fn failover_policy(&self) -> FailoverPolicy {
        self.failover_policy
    }
    /// Whether `node_id` is a replica for this database.
    pub fn is_replica(&self, node_id: NodeId) -> bool {
        self.replicas.contains(&node_id)
    }
    /// Whether a write to `node_id` is allowed. The primary is
    /// the only valid write target.
    pub fn allows_write(&self, node_id: NodeId) -> bool {
        node_id == self.primary_node_id
    }
}

/// Kind of shared storage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SharedStorageKind {
    /// NFS mount.
    Nfs,
    /// CephFS mount.
    Cephfs,
    /// GlusterFS mount.
    Glusterfs,
}

/// A shared storage declaration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SharedStorage {
    id: StorageId,
    kind: SharedStorageKind,
    mount_path: PathBuf,
    sites_attached: Vec<Uuid>,
    backups_attached: Vec<Uuid>,
}

impl SharedStorage {
    /// The canonical panel-managed mount root.
    pub const MOUNT_ROOT: &'static str = "/srv/openpanel/storage";

    /// Build a new shared storage declaration. The mount path
    /// MUST be under `MOUNT_ROOT` to keep the file hierarchy
    /// under the panel's control.
    pub fn new(
        id: StorageId,
        kind: SharedStorageKind,
        mount_path: PathBuf,
    ) -> Result<Self, ClusterError> {
        let canonical = mount_path
            .components()
            .filter(|c| !matches!(c, std::path::Component::CurDir))
            .collect::<PathBuf>();
        let root = Path::new(Self::MOUNT_ROOT);
        if !canonical.starts_with(root) {
            return Err(ClusterError::MountOutsidePanelRoot);
        }
        Ok(Self {
            id,
            kind,
            mount_path: canonical,
            sites_attached: Vec::new(),
            backups_attached: Vec::new(),
        })
    }
    /// Restore from persistence.
    pub fn restore(
        id: StorageId,
        kind: SharedStorageKind,
        mount_path: PathBuf,
        sites_attached: Vec<Uuid>,
        backups_attached: Vec<Uuid>,
    ) -> Self {
        Self {
            id,
            kind,
            mount_path,
            sites_attached,
            backups_attached,
        }
    }

    /// Storage id.
    pub fn id(&self) -> StorageId {
        self.id
    }
    /// Kind.
    pub fn kind(&self) -> SharedStorageKind {
        self.kind
    }
    /// Mount path.
    pub fn mount_path(&self) -> &Path {
        &self.mount_path
    }
    /// Site ids attached.
    pub fn sites_attached(&self) -> &[Uuid] {
        &self.sites_attached
    }
    /// Backup plan ids attached.
    pub fn backups_attached(&self) -> &[Uuid] {
        &self.backups_attached
    }
    /// Attach a site.
    pub fn attach_site(&mut self, site_id: Uuid) {
        if !self.sites_attached.contains(&site_id) {
            self.sites_attached.push(site_id);
        }
    }
    /// Attach a backup plan.
    pub fn attach_backup(&mut self, plan_id: Uuid) {
        if !self.backups_attached.contains(&plan_id) {
            self.backups_attached.push(plan_id);
        }
    }
}

/// Persistence port.
#[async_trait]
pub trait ClusterRepository: Send + Sync + 'static {
    /// Insert a new node.
    async fn insert_node(&self, node: &ClusterNode) -> Result<(), ClusterError>;
    /// Find a node by id.
    async fn find_node(&self, id: NodeId) -> Result<Option<ClusterNode>, ClusterError>;
    /// Update a node.
    async fn update_node(&self, node: &ClusterNode) -> Result<(), ClusterError>;
    /// List all nodes.
    async fn list_nodes(&self) -> Result<Vec<ClusterNode>, ClusterError>;
    /// Insert a storage declaration.
    async fn insert_storage(&self, storage: &SharedStorage) -> Result<(), ClusterError>;
    /// Find a storage declaration by id.
    async fn find_storage(&self, id: StorageId) -> Result<Option<SharedStorage>, ClusterError>;
    /// List all storage declarations.
    async fn list_storage(&self) -> Result<Vec<SharedStorage>, ClusterError>;
    /// Update a storage declaration.
    async fn update_storage(&self, storage: &SharedStorage) -> Result<(), ClusterError>;
    /// Insert a replicated database config.
    async fn insert_replicated(&self, db: &ReplicatedDatabase) -> Result<(), ClusterError>;
    /// Find a replicated database config by id.
    async fn find_replicated(&self, database_id: Uuid) -> Result<Option<ReplicatedDatabase>, ClusterError>;
    /// List all replicated database configs.
    async fn list_replicated(&self) -> Result<Vec<ReplicatedDatabase>, ClusterError>;
    /// Default impl to satisfy the placeholder pattern.
    async fn exists(&self, _id: NodeId) -> Result<bool, RepoError> {
        Ok(true)
    }
}

/// Errors that can occur in the cluster data model bounded context.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ClusterError {
    /// The replica list contains the primary node id.
    #[error("replica list contains the primary node id")]
    ReplicaIsPrimary,
    /// The mount path is outside the panel-managed root.
    #[error("mount path must be under /srv/openpanel/storage")]
    MountOutsidePanelRoot,
    /// The topology graph contains a cycle.
    #[error("topology contains a cycle")]
    TopologyCycle,
    /// The node is not found.
    #[error("node not found")]
    NodeNotFound,
    /// The shared storage declaration is not found.
    #[error("shared storage not found")]
    StorageNotFound,
    /// The replicated database config is not found.
    #[error("replicated database config not found")]
    ReplicatedDatabaseNotFound,
    /// Persistence failure.
    #[error("cluster persistence error: {0}")]
    Persistence(String),
}

impl From<RepoError> for ClusterError {
    fn from(error: RepoError) -> Self {
        ClusterError::Persistence(error.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn primary_node(id: NodeId) -> ClusterNode {
        ClusterNode::new(
            id,
            "fp",
            NodeRole::Primary,
            "us-east",
            "rack-1",
            Utc::now(),
        )
    }

    fn replica_node(id: NodeId, primary: NodeId) -> ClusterNode {
        ClusterNode::new(
            id,
            "fp",
            NodeRole::Replica { primary_node_id: primary },
            "us-east",
            "rack-2",
            Utc::now(),
        )
    }

    #[test]
    fn topology_detects_self_loop() {
        let id = NodeId::new();
        let mut node = ClusterNode::new(
            id,
            "fp",
            NodeRole::Replica { primary_node_id: id },
            "us-east",
            "rack-1",
            Utc::now(),
        );
        node.set_role(NodeRole::Replica { primary_node_id: id });
        let topo = ClusterTopology::new(vec![node]);
        assert!(topo.has_cycle());
    }

    #[test]
    fn topology_detects_three_node_cycle() {
        let a = NodeId::new();
        let b = NodeId::new();
        let c = NodeId::new();
        let mut nb = replica_node(b, a);
        let mut nc = replica_node(c, b);
        // Try to make `a` a replica of `c` to form a -> b -> c -> a.
        let mut na = replica_node(a, c);
        nb.set_role(NodeRole::Replica { primary_node_id: a });
        nc.set_role(NodeRole::Replica { primary_node_id: b });
        na.set_role(NodeRole::Replica { primary_node_id: c });
        let topo = ClusterTopology::new(vec![na, nb, nc]);
        assert!(topo.has_cycle());
    }

    #[test]
    fn topology_accepts_linear_chain() {
        let a = NodeId::new();
        let b = NodeId::new();
        let c = NodeId::new();
        let na = primary_node(a);
        let nb = replica_node(b, a);
        let nc = replica_node(c, a);
        let topo = ClusterTopology::new(vec![na, nb, nc]);
        assert!(!topo.has_cycle());
    }

    #[test]
    fn replica_rejects_primary_in_replica_list() {
        let a = NodeId::new();
        let b = NodeId::new();
        let err = ReplicatedDatabase::new(
            Uuid::new_v4(),
            a,
            vec![a, b],
            ReplicationMode::Async,
            FailoverPolicy::Auto,
        )
        .expect_err("must reject");
        assert_eq!(err, ClusterError::ReplicaIsPrimary);
    }

    #[test]
    fn allows_write_only_on_primary() {
        let a = NodeId::new();
        let b = NodeId::new();
        let cfg = ReplicatedDatabase::new(
            Uuid::new_v4(),
            a,
            vec![b],
            ReplicationMode::Async,
            FailoverPolicy::Auto,
        )
        .unwrap();
        assert!(cfg.allows_write(a));
        assert!(!cfg.allows_write(b));
        assert!(cfg.is_replica(b));
    }

    #[test]
    fn shared_storage_requires_panel_root() {
        let id = StorageId::new();
        let err = SharedStorage::new(
            id,
            SharedStorageKind::Nfs,
            PathBuf::from("/etc/passwd"),
        )
        .expect_err("must reject");
        assert_eq!(err, ClusterError::MountOutsidePanelRoot);
    }

    #[test]
    fn shared_storage_accepts_canonical_path() {
        let id = StorageId::new();
        let storage = SharedStorage::new(
            id,
            SharedStorageKind::Nfs,
            PathBuf::from("/srv/openpanel/storage/abc"),
        )
        .unwrap();
        assert_eq!(storage.mount_path(), Path::new("/srv/openpanel/storage/abc"));
    }
}
