# terminal-host-fleet Specification

## Purpose

aaPanel supports local and remote SSH terminal workflows, while BaoTa documents node management, remote service state, logs, jobs, databases, and terminal access. OpenPanel has agent, SSH-key, web-terminal, and host-security domains, but they are not presented as one safe operator workflow.
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

### Requirement: Host Capability Profiles Are Host-Scoped

Each managed host MUST expose an authenticated capability profile containing
OS/architecture, runtime adapters, supported operations, version, and health
state without exposing credentials or command material. The profile MUST be
bound to the same host identity the terminal and fleet scopes already use, so a
capability-guarded operation and a terminal session on the same host can be
correlated by target. An operation that requires a capability absent from the
host profile MUST be rejected before mutation with a remediation that names the
missing capability.

#### Scenario: Capability mismatch blocks the host operation

- **WHEN** an operator launches an operation that requires a capability the
  host profile does not declare
- **THEN** the operation is refused before mutation with a remediation that
  names the host identity and the missing capability, and no audit event carries
  credentials or command material.

### Requirement: Terminal Saved Servers Are Metadata-Only In Views

Saved remote servers (IP, port, SSH account, one
password-or-private-key credential, remarks) MUST render
metadata only in list and detail views: hostname/IP, port,
account name, and remarks. Password and private-key bytes MUST
never appear in HTML, logs, or audit metadata; mutations MUST
pass audit payloads through
`openpanel_core::audit::redact_metadata`.

#### Scenario: Server list redacts credentials

- **WHEN** an Owner saves a server with a password and a
  private key and opens the server list
- **THEN** the list shows IP, port, account, and remarks while
  containing neither credential string.

### Requirement: Saved Commands Carry Their Safety Classification

Every saved command MUST store the `host_fleet::classify_command`
result at save time and render it on every row: `Safe` or
`Dangerous(reason)`. Dispatch of a command with no cached
classification MUST be refused before any transport call.

#### Scenario: Dangerous command is labelled

- **WHEN** an Owner saves `systemctl restart openpanel`
- **THEN** the library row renders
  `Dangerous("Controls a system service")` with a confirmation
  gate.

#### Scenario: Unclassified command is refused

- **WHEN** a dispatch request names a command with no cached
  classification
- **THEN** the handler refuses before transport and emits no
  command body in audit.

### Requirement: Terminal Session Expiry Is Visible And Enforced

The `/terminal` local-session panel MUST render the
`SessionSummary` host context, connection state, and expiry,
and MUST refuse dispatch once `is_expired(now)` holds while
offering reconnect/terminate actions.

#### Scenario: Expired session refuses dispatch

- **WHEN** the session passes its expiry time
- **THEN** commands are rejected and the UI shows
  reconnect/terminate actions.

