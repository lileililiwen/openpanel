# operations-dashboard Specification

## ADDED Requirements

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
