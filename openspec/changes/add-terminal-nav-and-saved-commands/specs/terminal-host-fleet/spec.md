# terminal-host-fleet Specification

## ADDED Requirements

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
