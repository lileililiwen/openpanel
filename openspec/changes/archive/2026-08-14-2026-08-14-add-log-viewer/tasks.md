# Add Log Viewer — Tasks

## 1. Testing

- [x] 1.1 Unit: RBAC authorization matrix (own site, foreign site,
      audit, system); `LogQuery` parsing of `tail` + `filter`.
- [x] 1.2 Property: every returned line matches the authorized source;
      `tail(N)` returns exactly the last `N` lines in chronological
      order.
- [x] 1.3 Service: site / audit / system queries over a mock JSONL
      store; download triggers an audit event.
- [x] 1.4 Integration: an Owner reads their own site logs and is denied
      another Owner's site; a Server Admin reads the audit log; a
      `filter` narrows results.
- [ ] 1.5 CLI E2E: `openpanel logs site <id> --tail 50`.
- [ ] 1.6 Web: Logs page with kind tabs (access/error/audit/system),
      search box, tail toggle, and download button.

## 2. Domain and Application

- [x] 2.1 Implement `LogSource`, `LogQuery`, `LogLine`, `LogRange`
      under `crates/openpanel-domain/src/log_viewer/`.
- [x] 2.2 Implement `LogAggregator`, `LogReader`, `LogAuthorization`;
      register via `ModuleRegistry`.
- [x] 2.3 Wire the aggregator to read the JSONL export produced by
      `observability-export`.

## 3. Adapters and UI

- [ ] 3.1 Add `GET /logs/sites/{id}`, `GET /logs/audit`,
      `GET /logs/system/{service}` REST routes.
- [ ] 3.2 Add `openpanel logs {site,audit,system}`.
- [ ] 3.3 Build the Logs page (kind tabs, search, tail, download) with
      RBAC scoping in the UI.

## 4. Validation

- [x] 4.1 `cargo test --workspace` twice.
- [x] 4.2 `make check` clean.
- [x] 4.3 Smoke-test: as an Owner, view own site logs and confirm a
      foreign site is denied; as Server Admin, view the audit log.
- [x] 4.4 Archive with `openspec archive add-log-viewer`.
