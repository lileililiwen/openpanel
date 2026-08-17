# Add cluster data model

## Why

`openspec/.../add-multi-host-agent` introduces a control-plane
protocol and per-host registration. The proposal assumes a
flat fleet with one control plane per host set. A real
multi-host deployment also needs **role topology** (primary /
replica / edge), **shared-storage declarations**, and
**replicated databases**. This change adds the formal machine-
readable description of a host's role so the panel can route
recipes, queries, and traffic correctly across the fleet.

## What Changes

- New bounded context `cluster-data-model` carrying
  `ClusterNode`, `ClusterTopology`, `SharedStorage`,
  `ReplicatedDatabase`.
- New endpoints:
  `GET /cluster/topology`,
  `POST /cluster/topology/{node_id}/role`,
  `POST /cluster/shared-storage`,
  `POST /cluster/databases/{id}/replication`.
- Coupling: a `ClusterNode` row joins an agent registration
  (from `add-multi-host-agent`) with a typed role and topology.
- A replicated database is declared by pointing a
  `ReplicatedDatabase` at a primary (`Source = Primary(id)`)
  and zero or more replicas; the panel migrates read traffic
  accordingly.

## Capabilities

### New Capabilities

- `cluster-data-model`: typed host roles, shared storage, and
  replicated databases.

## Impact

- Domain: `ClusterNode`, `ClusterTopology`, `SharedStorage`,
  `ReplicatedDatabase`.
- App: `ClusterService`, replication adapter for MySQL Group
  Replication / Galera, NFS / CephFS shared-storage adapters.
- API/CLI/web: `/cluster/*`; CLI `openpanel cluster {topology,role,storage,replication}`;
  web `/cluster/topology`.
- Coupling: depends on the existing `add-multi-host-agent`
  proposal and the `databases` cap for replicated-database
  migration.
