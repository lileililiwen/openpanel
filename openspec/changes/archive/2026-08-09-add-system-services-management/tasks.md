## 1. Testing

- [x] 1.1 Unit-test every public descriptor/state/action/health transition and capability-impact calculation.
- [x] 1.2 Property-test descriptor IDs never become command arguments and health hysteresis/attempt budgets remain bounded.
- [x] 1.3 Service-test action authorization, timeout, readiness, actual-state refresh, auto-restart cooldown, alerts, logs, and audit with mocks.
- [x] 1.4 Integration-test every REST/web route for supported/unsupported hosts, CSRF, confirmation, RBAC, and redaction.
- [x] 1.5 CLI E2E-test list/status/preview/start/stop/restart/reload/enable/disable/logs using a fake controller.

## 2. Implementation

- [x] 2.1 Implement service domain, controller/probe/journal ports, repositories, and migrations.
- [x] 2.2 Add fixed-descriptor systemd adapter, health task, hysteresis, alerts, auto-restart budget, and audit.
- [x] 2.3 Add REST, CLI, `/services` pages, impact confirmation, and navigation registration.

## 3. Validation

- [x] 3.1 Run `cargo test --workspace` twice and disposable-systemd adapter tests where available.
- [x] 3.2 Run `make check`, smoke-test status/reload/health recovery, and archive with OpenSpec.
