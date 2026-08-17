# Add account hierarchy — Tasks

## 1. Testing

- [x] 1.1 Unit tests for cycle detection, pool math, tree flatten.
- [x] 1.2 Property tests: ≤ 1 parent, no cycles, pool ≥ child claims.
- [x] 1.3 Service tests with mock resolver and audit for
      create-child / delete / pool operations.
- [x] 1.4 Integration: tree + pool lifecycle through REST.
- [ ] 1.5 CLI E2E: tree, create-child, delete-with-children-fails. (deferred)
- [ ] 1.6 Web: `/accounts/tree` with usage bars. (deferred)

## 2. Domain and Application

- [x] 2.1 Implement `AccountRelationship`, `QuotaPool`,
      `HierarchyNode` under
      `crates/openpanel-domain/src/account_hierarchy/`.
- [x] 2.2 Add SQLite migrations for `account_relationships` and
      `quota_pools`.
- [x] 2.3 Implement `HierarchyService` and register the module.

## 3. Adapters and UI

- [x] 3.1 Add `/api/v1/users/{id}/children`, `/api/v1/users/{id}/tree`,
      `/api/v1/users/{id}/quota-pool` endpoints.
- [ ] 3.2 Add `openpanel user create-child`,
      `openpanel pool set`, `openpanel pool usage`. (deferred)
- [ ] 3.3 Build `/accounts/tree` web page with usage bars. (deferred)

## 4. Validation

- [x] 4.1 `cargo test --workspace` twice.
- [x] 4.2 `make check` clean (modulo pre-existing clippy/doc nits).
- [x] 4.3 Smoke-test: a parent with a 1TB bandwidth pool and
      three children each claiming 300GB; the fourth child is
      rejected with `pool_exhausted`.
- [x] 4.4 Archive with `openspec archive add-account-hierarchy`.
