# Add cluster data model — Design

## Topology shape

```rust
pub enum NodeRole {
    Primary,
    Replica { primary_node_id: NodeId },
    Edge,
}

pub struct ClusterNode {
    pub node_id: NodeId,
    pub host_fingerprint: String,
    pub role: NodeRole,
    pub region: String,
    pub rack: String,
    pub labels: BTreeMap<String, String>,
    pub last_seen_at: DateTime<Utc>,
}

pub struct ClusterTopology {
    pub nodes: Vec<ClusterNode>,
    pub edges: Vec<TopologyEdge>,         // parent → child primary/replica
}
```

A `ReplicatedDatabase` carries:

```rust
pub struct ReplicatedDatabase {
    pub db_id: DatabaseId,
    pub primary_node_id: NodeId,
    pub replicas: Vec<NodeId>,
    pub replication_mode: ReplicationMode,     // Async | SemiSync | Sync
    pub failover_policy: FailoverPolicy,       // Auto | Manual
}
```

## Routing policy

- Reads to a `ReplicatedDatabase` route to the nearest replica
  by region, falling back to the primary if no replica is
  healthy.
- Writes always go to the primary; the panel refuses to
  register a replica as a write target.

## Shared storage

```rust
pub struct SharedStorage {
    pub id: StorageId,
    pub kind: SharedStorageKind,    // Nfs | Cephfs | Glusterfs
    pub mount_path: PathBuf,
    pub sites_attached: Vec<SiteId>,
    pub backups_attached: Vec<PlanId>,
}
```

Mount paths are owned exclusively by the panel under
`/srv/openpanel/storage/<id>` and referenced by document_root
overlays; raw symlinks are not allowed.

## Endpoints

```
GET   /api/v1/cluster/topology
POST  /api/v1/cluster/topology/{node_id}/role    body: NodeRole
POST  /api/v1/cluster/shared-storage             body: SharedStorageCreate
GET   /api/v1/cluster/shared-storage
POST  /api/v1/cluster/databases/{id}/replication body: ReplicatedDatabaseCreate
GET   /api/v1/cluster/databases/{id}/replication
```

## CLI

```
openpanel cluster topology show
openpanel cluster role set  <node_id> primary|replica|edge --primary <id>
openpanel cluster storage   add  --kind nfs --mount <p>
openpanel cluster database  <db_id> replicate --replicas <id>,<id>
```

## Tests

```
1.1  Unit: NodeRole validation; SharedStorage mount under
      /srv/openpanel/storage only; topology graph builder
      rejects cycles.
1.2  Property: write-path never returns a replica id; reads
      deterministically route by region.
1.3  Service tests with mock agents and adapters.
1.4  Integration: declare topology, attach storage, replicate
      a test DB across two nodes.
1.5  CLI E2E.
1.6  Web: /cluster/topology SVG with role badges.
```
