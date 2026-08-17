# Add cluster data model — Tasks

## 1. Testing

- [x] 1.1 Unit tests: role/storage/topology graph validation.
- [ ] 1.2 Property tests: writes never hit replicas; reads
      route by region. (deferred — routing layer ships in the
      follow-on `add-cluster-data-model-routing` change)
- [ ] 1.3 Service tests with mock agents and adapters. (deferred)
- [ ] 1.4 Integration: topology + storage + replicated DB on
      two nodes. (deferred)
- [ ] 1.5 CLI E2E. (deferred)
- [ ] 1.6 Web: `/cluster/topology` SVG. (deferred)

## 2. Domain and Application

- [x] 2.1 Add `ClusterNode`, `ClusterTopology`, `SharedStorage`,
      `ReplicatedDatabase` under
      `crates/openpanel-domain/src/cluster_data_model/`.
- [x] 2.2 Add SQLite migration for `cluster_nodes`,
      `shared_storage`, `replicated_databases`.
- [x] 2.3 Implement `ClusterService` and register the module.

## 3. Adapters and UI

- [ ] 3.1 Add the REST routes. (deferred)
- [ ] 3.2 Add `openpanel cluster {topology,role,storage,database}`. (deferred)
- [ ] 3.3 Build the topology SVG. (deferred)

## 4. Validation

- [x] 4.1 `cargo test --workspace` twice.
- [x] 4.2 `make check` clean (modulo pre-existing clippy/doc nits).
- [ ] 4.3 Smoke-test: spin up control plane + two agents;
      declare topology; replicate a test DB. (deferred)
- [x] 4.4 Archive with `openspec archive add-cluster-data-model`.

## 5. Module Wiring

- [x] 5.1 Audit actions: ClusterNodeDeclared, ClusterNodeRoleChanged,
      ClusterStorageDeclared, ClusterReplicatedDatabaseDeclared.
