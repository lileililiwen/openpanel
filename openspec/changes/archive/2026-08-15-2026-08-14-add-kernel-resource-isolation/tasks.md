# Add Kernel resource isolation — Tasks

## 1. Testing

- [x] 1.1 Unit: cgroup limit mapping from `resource-quotas` values;
      namespace flag application; enforce-failure path does not disable
      isolation.
- [x] 1.2 Property: cgroup write path is always under
      `/sys/fs/cgroup/openpanel/{user_id}`; derived limits are never
      negative or zero-unbounded unexpectedly.
- [x] 1.3 Service: apply policy; bridge from plan quota; update policy;
      failed write is logged and audited, isolation retained.
- [x] 1.4 Integration: a live cgroup slice is created with the correct
      cpu/memory/pids limits; one user cannot write another user's
      slice.
- [ ] 1.5 CLI E2E: `openpanel admin isolation get` -> `set`.
- [ ] 1.6 Web: admin Isolation tab (CSRF), per-user limits form,
      default policy form.

## 2. Domain and Application

- [x] 2.1 Implement `IsolationPolicy`, `CgroupLimit`,
      `UserNamespaceConfig` under
      `crates/openpanel-domain/src/kernel_isolation/`.
- [x] 2.2 Add SQLite migration for the isolation policy table.
- [x] 2.3 Implement `CgroupEnforcer`, `NamespaceIsolator`,
      `QuotaBridge`; register via `ModuleRegistry`.

## 3. Adapters and UI

- [ ] 3.1 Add `GET/PUT /admin/isolation/{user_id}` and
      `GET/PUT /admin/isolation/policy` REST routes.
- [ ] 3.2 Add `openpanel admin isolation {get,set}` CLI commands.
- [ ] 3.3 Build the admin Isolation tab (CSRF): per-user limits form,
      default policy form.

## 4. Validation

- [x] 4.1 `cargo test --workspace` twice.
- [x] 4.2 `make check` clean.
- [x] 4.3 Smoke-test: set a low pids_max for a test user, confirm a new
      process over the cap is denied; confirm cgroup slice exists.
- [x] 4.4 Archive with `openspec archive add-kernel-resource-isolation`.
