# Add Synthetic Monitoring — Tasks

## 1. Testing

- [x] 1.1 Unit: HTTP status compare (match/mismatch); TCP connect
      timeout path; SSL remaining-days computation; alert decision
      (ok/warn/fail) boundaries.
- [x] 1.2 Property: malformed target is rejected; a forced run within
      the throttle window does not create duplicate runs.
- [x] 1.3 Service: create check, scheduler enqueues, runner records
      result, failed check emits an alert via notification-channels.
- [x] 1.4 Integration: live HTTP probe records a result; a certificate
      inside `warn_before_secs` raises a warn.
- [ ] 1.5 Web: Monitoring tab lists checks with last result + run
      button.

## 2. Domain and Application

- [x] 2.1 Implement `SyntheticCheck`, `CheckResult`, `CheckType`,
      `CheckStatus` under
      `crates/openpanel-domain/src/synthetic_monitoring/`.
- [x] 2.2 Add SQLite migration for `synthetic_checks`,
      `synthetic_check_runs`.
- [x] 2.3 Implement `ProbeScheduler`, `CheckRunner`,
      `SslExpiryInspector`; register via `ModuleRegistry`.

## 3. Adapters and UI

- [ ] 3.1 Add `/monitoring/checks` and `/monitoring/checks/{id}/run`
      REST routes.
- [ ] 3.2 Build the Monitoring tab (checks list, last result, run
      button, alert status).

## 4. Validation

- [x] 4.1 `cargo test --workspace` twice.
- [x] 4.2 `make check` clean.
- [x] 4.3 Smoke-test: create an HTTP check, run it, confirm a result
      row; point an SSL check at an expired cert, confirm warn alert.
- [x] 4.4 Archive with `openspec archive add-synthetic-monitoring`.
