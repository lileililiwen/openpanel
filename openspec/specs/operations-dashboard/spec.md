# operations-dashboard Specification

## Purpose
TBD - created by archiving change redesign-operations-dashboard. Update Purpose after archive.
## Requirements
### Requirement: Dashboard exposes operational health

The dashboard MUST show server identity, resource health, service state, security posture, backup/job attention, and last-updated information using existing services.

#### Scenario: Healthy owner dashboard

- **WHEN** an owner loads the dashboard
- **THEN** host health, attention items, resource links, and update time are visible

### Requirement: Failures are not presented as healthy zeros

The dashboard MUST distinguish unavailable, stale, and zero measurements.

#### Scenario: Monitoring collection fails

- **WHEN** a monitoring service call fails
- **THEN** the affected widget shows an error or unavailable state with retry guidance

### Requirement: Dashboard respects scope

Dashboard resource counts and links MUST be limited to the caller's role and ownership scope.

#### Scenario: User loads dashboard

- **WHEN** a user loads the dashboard
- **THEN** host-owner controls are absent and resource data is user-scoped

### Requirement: Dashboard View Is Split From Its Tests

The operations dashboard view SHALL live at
`crates/openpanel-web/src/dashboard/mod.rs`, with its unit tests in the
sibling `crates/openpanel-web/src/dashboard/tests.rs`. Neither file SHALL
exceed the `file-length` `hard_limit` configured in
`cargo-lint-extra.toml`.

#### Scenario: View and tests live in separate files

- **WHEN** a contributor edits how the dashboard composes its widgets
- **THEN** the implementation is in `dashboard/mod.rs` and its covering
  tests are in `dashboard/tests.rs`.

#### Scenario: The view grows past the hard limit

- **WHEN** `dashboard/mod.rs` exceeds the configured `hard_limit`
- **THEN** `make file-length` fails and names the file.

