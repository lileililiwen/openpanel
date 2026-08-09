## 1. Testing

- [x] 1.1 Add unit tests for every public schedule/job constructor and transition, including DST gaps/folds, invalid fields, timeout bounds, and overlap states.
- [x] 1.2 Add property tests proving parsed schedules always advance, invalid paths never escape owned roots, and terminal run states cannot transition.
- [x] 1.3 Add service tests with mocked clock, repository, process runner, audit, and HTTP client for leasing, concurrency, timeout, recovery, output caps, and RBAC.
- [x] 1.4 Add one integration test per REST route covering success, validation, ownership, missing ID, and unauthenticated cases.
- [x] 1.5 Add CLI E2E tests for create/list/get/update/enable/disable/delete/run/runs using isolated state and deterministic clocks.
- [x] 1.6 Add web integration tests for list/form/actions/history, CSRF, escaped output, and role filtering.

## 2. Domain and Application

- [x] 2.1 Implement cron domain values, aggregate, errors, and repository/process/clock ports.
- [x] 2.2 Add SQLite migrations/repository with atomic leases and retention queries.
- [x] 2.3 Implement service use cases and the supervised scheduler with limits, recovery, and audit.

## 3. Adapters and UI

- [x] 3.1 Add REST DTOs/routes and typed error mapping.
- [x] 3.2 Add complete `openpanel cron` commands and handlers.
- [x] 3.3 Add `/cron` web pages and register the module/navigation capability.

## 4. Validation

- [x] 4.1 Run `cargo test --workspace` twice, including paused-time scheduler tests.
- [x] 4.2 Run `make check` and smoke-test create -> run-now -> history -> disable.
- [x] 4.3 Archive the change with OpenSpec after all tasks pass.
