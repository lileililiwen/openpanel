# cluster-data-model Specification

## Purpose

TBD - created by archiving change add-cluster-data-model.

## Requirements

### Requirement: Node Role Topology

The cluster-data-model bounded context SHALL model a `ClusterNode` aggregate with `NodeId` (UUID), `host_fingerprint`, `NodeRole` (`Primary | Replica { primary_node_id } | Edge`), `region`, `rack`, `labels`, and `last_seen_at`. The `ClusterTopology::has_cycle` helper SHALL detect any self-loop (`Replica` referencing itself) and any transitive cycle in the replica-to-primary edge graph.

#### Scenario: Self-loop is detected

- **WHEN** a node is created with `NodeRole::Replica { primary_node_id: self.id }`
- **THEN** `ClusterTopology::has_cycle` returns `true`.

#### Scenario: Linear chain is accepted

- **WHEN** the topology is `primary -> replica_a -> replica_b` (both replicas reference the same primary)
- **THEN** `ClusterTopology::has_cycle` returns `false`.

### Requirement: Replicated Database

The bounded context SHALL model a `ReplicatedDatabase` carrying `database_id`, `primary_node_id`, `replicas`, `replication_mode` (`Async | SemiSync | Sync`), and `failover_policy` (`Auto | Manual`). The constructor SHALL reject any replica list that contains the primary node id.

#### Scenario: Constructor rejects self-reference

- **WHEN** a `ReplicatedDatabase` is built with `replicas = [primary_node_id, ...]`
- **THEN** construction fails with `ClusterError::ReplicaIsPrimary`.

#### Scenario: Write path is enforced

- **WHEN** a write is dispatched to a replica node
- **THEN** `allows_write(replica_id)` returns `false`; only the primary is a valid write target.

### Requirement: Shared Storage

The bounded context SHALL model a `SharedStorage` declaration with `StorageId`, `SharedStorageKind` (`Nfs | Cephfs | Glusterfs`), `mount_path`, `sites_attached`, and `backups_attached`. The constructor SHALL reject any mount path that does not fall under `/srv/openpanel/storage` to keep the file hierarchy under the panel's control.

#### Scenario: Mount path outside panel root is rejected

- **WHEN** a `SharedStorage` is created with `mount_path = /etc/passwd`
- **THEN** construction fails with `ClusterError::MountOutsidePanelRoot`.

#### Scenario: Mount path under panel root is accepted

- **WHEN** a `SharedStorage` is created with `mount_path = /srv/openpanel/storage/abc`
- **THEN** construction succeeds.

### Requirement: Audit and Event Surface

Every cluster lifecycle mutation SHALL emit an audit event with the actor, the affected node/storage/database id, and one of `{ClusterNodeDeclared, ClusterNodeRoleChanged, ClusterStorageDeclared, ClusterReplicatedDatabaseDeclared}`.

#### Scenario: Role change is audited

- **WHEN** an Owner changes a node's role to `Replica`
- **THEN** the audit log records `ClusterNodeRoleChanged{actor, node_id, role="replica"}`.
