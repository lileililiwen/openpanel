## Purpose

Complements `add-multi-host-agent` with the missing
machine-readable description of a host's role, the shared-
storage subsystem, and replicated databases. With this model
the control plane can route reads and writes, fail over, and
treat storage as a managed first-class resource.

# cluster-data-model Specification

## Requirements

### Requirement: ClusterNode and Role

The system SHALL model a `ClusterNode` per registered agent
with a `NodeRole` of `Primary | Replica | Edge`. A `Replica`
MUST point at a Primary; chains are forbidden. Each role
governs what recipes the node may execute and what queries
the control plane may dispatch.

#### Scenario: Replica requires primary

- **WHEN** an Owner attempts to set `role = Replica` without a valid `primary_node_id`
- **THEN** the request is rejected with `ClusterError::MissingPrimary`.

#### Scenario: Chain rejected

- **WHEN** an Owner attempts to set `replica.primary_node_id = <another replica id>`
- **THEN** the request is rejected with `ClusterError::ReplicaChainForbidden`.

#### Scenario: Topology has no cycles

- **WHEN** the topology graph is recomputed after a role change
- **THEN** any cycle is detected and the change is rejected with the cycle path.

### Requirement: Read Routing by Region

`ClusterService` SHALL route reads for a `ReplicatedDatabase`
to the nearest replica by region (literal-string match in the
topology labels), falling back to the primary when no replica
is healthy. Writes SHALL always target the primary.

#### Scenario: Replica read

- **WHEN** a User-role request targets a database whose topology has a replica in the same region as the request
- **THEN** the read is served from that replica; audit `ClusterReadRoutedToReplica` records the replica id only.

#### Scenario: Replica down

- **WHEN** the nearest replica is unhealthy
- **THEN** the read is served from the primary; audit `ClusterReadFallbackToPrimary` is recorded.

#### Scenario: Write to replica refused

- **WHEN** a write is attempted against a replica
- **THEN** the request is rejected with `ClusterError::WritesMustTargetPrimary`.

### Requirement: SharedStorage Lifecycle

`SharedStorage{ kind, mount_path }` records shall be
declared, listed, and deleted. Mount paths MUST be located
under `/srv/openpanel/storage/<id>`; raw symlinks are
rejected. A site or backup plan can be attached to a
`SharedStorage`, which means its working dir is co-located
with that storage.

#### Scenario: Mount outside panel path refused

- **WHEN** an Owner submits `{ mount_path: "/mnt/cephfs" }`
- **THEN** the request is rejected with `SharedStorageError::MountOutsidePanelPath`.

#### Scenario: Symlink refused

- **WHEN** the mount path is a symlink to a path outside the panel storage tree
- **THEN** the request is rejected with `SharedStorageError::SymlinkForbidden`.

### Requirement: ReplicatedDatabase Migration

`ReplicatedDatabase` SHALL declare a primary and a set of
replicas with a `ReplicationMode` and a `FailoverPolicy`. The
panel SHALL migrate any in-flight writes to the primary
before flagging the source as `Replicated`.

#### Scenario: Initial replication

- **WHEN** an Owner activates a replication of db `d1` with two replicas
- **THEN** the source is set to `Replicated(Async)` by default;
        audit `DatabaseReplicationInitialized` records the db
        and replica ids only.

#### Scenario: Failover policy auto

- **WHEN** `FailoverPolicy=Auto` is configured and the primary is unhealthy for ≥ 60s
- **THEN** the panel promotes the replica marked `primary_candidate=true` and audit `DatabaseFailoverAuto`.

#### Scenario: Manual failover

- **WHEN** `FailoverPolicy=Manual` and the primary is unhealthy
- **THEN** the panel refuses to auto-promote; only an Owner
        can promote via `POST /cluster/databases/{id}/failover`
        with a typed `confirmed_at`.

### Requirement: Topology Web Rendering

The web UI SHALL render `/cluster/topology` as an SVG with
role-coloured nodes and primary/replica edges. The page MUST
be readable on a non-JS browser (server-rendered SVG) and
MUST be keyboard-navigable.

#### Scenario: SVG render

- **WHEN** an Owner opens `/cluster/topology`
- **THEN** the page renders the SVG with role-coloured
        labels and key-navigation support.

#### Scenario: Detailed view

- **WHEN** a node is selected
- **THEN** the side panel shows the role, region, last_seen,
        and any attached sites or replicated databases.
