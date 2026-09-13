# Tasks: Complete backup DR and migration operations

## 1. Testing

- [x] Add remote-target tests for connectivity, credentials rejection, write/read verification, retention, and unavailable storage.
- [x] Add health projection tests for last success, next run, RPO, RTO, stale state, and failed state.
- [x] Add restore-drill tests proving success, corruption detection, notification, report retention, and sandbox teardown on failure.
- [x] Add migration round-trip tests for manifest compatibility, collision preview, bootstrap, and audit events on both hosts.
- [x] Run the new tests red before implementation.

## 2. Implementation

- [x] Add remote target verification and backup-health projection using existing repositories.
- [x] Add drill scheduling/history/alerting surfaces.
- [x] Add restore progress and scoped collision/recovery UI/API/CLI flows.
- [x] Complete host migration export/import/bootstrap orchestration.
- [x] Add operator documentation for RPO/RTO and recovery runbooks.

## 3. Verification

- [x] Run focused backup, snapshot, migration, notification, and web tests.
- [x] Run `make check`.
- [x] Run `openspec validate complete-backup-dr-and-migration-operations --strict`.
- [ ] Run an environment-backed restore drill when storage and database prerequisites exist.
