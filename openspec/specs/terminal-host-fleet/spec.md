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

