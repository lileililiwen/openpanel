# Add scheduled maintenance windows — Tasks

## 1. Testing

- [x] 1.1 Unit tests: action-class discrimination; window
      bound; override TTL.
- [x] 1.2 Property tests: enforcer behavior; override single-
      use.
- [x] 1.3 Service tests with mock scheduler and audit.
- [x] 1.4 Integration: window active + destructive call
      blocked.
- [ ] 1.5 CLI E2E.
- [ ] 1.6 Web: `/admin/maintenance` editor (CSRF); upcoming
      banner.

## 2. Domain and Application

- [x] 2.1 Add `MaintenanceWindow`, `MaintenanceOverride`,
      `DestructiveActionClass` under
      `crates/openpanel-domain/src/maintenance_windows/`.
- [x] 2.2 Add SQLite migration for `maintenance_windows`,
      `maintenance_overrides`.
- [x] 2.3 Implement `MaintenanceEnforcer` and integrate with
      the relevant bounded contexts.

## 3. Adapters and UI

- [ ] 3.1 Add the REST routes.
- [ ] 3.2 Add the CLI subcommands.
- [ ] 3.3 Build the editor and an upcoming-window banner.

## 4. Validation

- [x] 4.1 `cargo test --workspace` twice.
- [x] 4.2 `make check` clean.
- [x] 4.3 Smoke-test: declare a 5-minute window; an install
      call is blocked; override is consumed once.
- [x] 4.4 Archive with `openspec archive add-scheduled-maintenance-windows`.
