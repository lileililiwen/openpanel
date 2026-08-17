-- Cluster data model v0.1 initial schema.
-- Owned by the cluster-data-model bounded context. The roles_json
-- column on cluster_nodes stores the NodeRole variant as a typed
-- JSON blob; the topology graph is built by joining cluster_nodes
-- on the primary_node_id field of any replica role.

CREATE TABLE IF NOT EXISTS cluster_nodes (
    id TEXT PRIMARY KEY,
    host_fingerprint TEXT NOT NULL,
    role_json TEXT NOT NULL,
    region TEXT NOT NULL,
    rack TEXT NOT NULL,
    labels_json TEXT NOT NULL,
    last_seen_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_cluster_nodes_role ON cluster_nodes(role_json);
CREATE INDEX IF NOT EXISTS idx_cluster_nodes_region ON cluster_nodes(region);

CREATE TABLE IF NOT EXISTS shared_storage (
    id TEXT PRIMARY KEY,
    kind TEXT NOT NULL,
    mount_path TEXT NOT NULL,
    sites_attached_json TEXT NOT NULL,
    backups_attached_json TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS replicated_databases (
    database_id TEXT PRIMARY KEY,
    primary_node_id TEXT NOT NULL,
    replicas_json TEXT NOT NULL,
    replication_mode TEXT NOT NULL,
    failover_policy TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_replicated_databases_primary
    ON replicated_databases(primary_node_id);
