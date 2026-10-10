# operations-dashboard Specification

## Purpose

The current dashboard contains host gauges, five counts, and recent alerts. aaPanel presents server identity, operating system, CPU, memory, disk, network, website/database counts, security risks, and software status on the home page. OpenPanel's security and reliability strengths are not visible at first use.
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

### Requirement: Clickable Resource Counts Including FTP

The dashboard MUST render Site, FTP, and Database counts as links to
their workflows (`/sites`, `/ftp`, `/databases`), scoped to the caller.
FTP counts MUST come from the existing FTP service; a collection failure
MUST render an error state, never a silent zero.

#### Scenario: Owner clicks through counts

- **WHEN** an owner clicks the FTP count on the dashboard
- **THEN** the browser navigates to the FTP workflow scoped to the
  caller's visible sites.

#### Scenario: FTP collection fails

- **WHEN** the FTP service call fails
- **THEN** the FTP card shows an unavailable state with retry guidance,
  not zero.

### Requirement: Security-Risk Card With One-Click Fix

The dashboard MUST show a security-risk card projecting the open
findings count from the existing control-plane service, with a one-click
fix link to `/security/findings`. Evidence MUST stay redacted; the card
MUST NOT introduce a scanner.

#### Scenario: Open findings link to queue

- **WHEN** open findings exist
- **THEN** the card shows the count and its fix link opens the findings
  queue.

### Requirement: Software Shortcuts With Display Toggle

The dashboard MUST show software shortcuts drawn from the existing
catalog search, filtered by a per-user Display-on-dashboard toggle. The
toggle MUST NOT mutate the catalog.

#### Scenario: Toggle filters shortcuts

- **WHEN** a user enables Display-on-dashboard on a subset of entries
- **THEN** only that subset renders as dashboard shortcuts.

### Requirement: Traffic Summary And Panel Shortcuts

The dashboard MUST show a traffic summary aggregating the existing
per-interface network rows, plus panel Update/Fix/Restart shortcuts that
deep-link to existing routes. The change MUST NOT add monitoring
collection or a self-update engine.

#### Scenario: Traffic and panel actions resolve

- **WHEN** an owner views the traffic summary and panel shortcuts
- **THEN** totals match the underlying rows and each shortcut resolves
  to a mounted route.

### Requirement: Sub-User Quota Summary

The dashboard MUST show a role-scoped sub-user quota summary (QPS,
bandwidth, traffic, disk) projected from existing quota surfaces.

#### Scenario: User sees own quota scope

- **WHEN** a non-owner views the dashboard
- **THEN** only quota within the caller's scope is shown.

