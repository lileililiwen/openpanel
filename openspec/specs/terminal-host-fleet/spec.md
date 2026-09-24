# terminal-host-fleet Specification

## Purpose
TBD - created by archiving change add-terminal-and-host-fleet-ux. Update Purpose after archive.
## Requirements
### Requirement: Terminal sessions are host-scoped

Each browser terminal session MUST identify one host, one actor, one scope, and one expiry time; output MUST never cross sessions.

#### Scenario: Owner opens terminal

- **WHEN** an owner starts a terminal session
- **THEN** the host context, connection state, and expiry are visible

### Requirement: Terminal secrets are protected

Private keys, tokens, passwords, and secret command values MUST NOT be returned to the browser or written to logs/audit metadata.

#### Scenario: Terminal connection fails

- **WHEN** a host connection fails
- **THEN** the error contains safe diagnostics without credential material

### Requirement: Expired sessions terminate

Inactive or explicitly terminated terminal sessions MUST stop accepting commands and provide a clear reconnect path.

#### Scenario: Session expires

- **WHEN** the session exceeds its inactivity limit
- **THEN** commands are rejected and the UI shows reconnect/terminate actions

### Requirement: Deployment Adapter Operations Are Per-Host and Auditable

A deployment adapter that targets a host MUST treat the operation as a
host-scoped, auditable action whose evidence, plan target, and audit
event all name the same host identity. The terminal and fleet scopes
already identify one host per session; the deployment-adapter service
MUST reuse the same `target` identity and the same `AuditService` so a
host-scoped terminal session and a host-scoped adapter run can be
correlated by target without leaking transport material or secrets.

#### Scenario: Adapter run is bound to a host target

- **WHEN** an operator launches a deployment adapter action against a
  host that has an open terminal session
- **THEN** the resulting `SoftwareChanged` audit event names the same
  `target` as the terminal session, the evidence record's `target`
  field matches, and the audit metadata never carries the transport
  command, the host path, or any secret value.

