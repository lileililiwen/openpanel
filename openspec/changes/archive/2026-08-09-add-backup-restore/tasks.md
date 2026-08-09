## 1. Testing

- [x] 1.1 Unit-test every public plan/run/manifest transition, retention decision, conflict policy, and format-version check.
- [x] 1.2 Property-test manifest round trips, checksum tamper detection, resource ownership filtering, and archive paths never escaping restore staging.
- [x] 1.3 Service-test successful/partial runs, cancellation, crash cleanup, disk shortage, dump failure, atomic finalization, restore preview/overwrite, secret redaction, and RBAC with mocks.
- [x] 1.4 Integration-test each backup/restore HTTP route and web action for success, CSRF, validation, authorization, corrupt artifacts, and progress.
- [x] 1.5 CLI E2E-test plan CRUD, run, list, verify, restore preview/start, status, and delete in isolated directories.

## 2. Domain and Application

- [x] 2.1 Implement backup aggregates, manifest, errors, repository, destination, archiver, dumper, and clock ports.
- [x] 2.2 Add migrations/repositories plus streaming local destination and MySQL/site capture adapters.
- [x] 2.3 Implement orchestration, checksum/finalization, retention, restore preflight/apply, cancellation, audit, and Cron linkage.

## 3. Surfaces and Validation

- [x] 3.1 Add REST, CLI, `/backups` web UI, progress views, and navigation registration.
- [x] 3.2 Run `cargo test --workspace` twice and fault-injection tests for every staging/finalization boundary.
- [x] 3.3 Run `make check`, smoke-test backup -> verify -> restore preview, then archive with OpenSpec.
