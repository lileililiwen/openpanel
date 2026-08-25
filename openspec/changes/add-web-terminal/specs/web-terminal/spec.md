## ADDED Requirements

### Requirement: One-Time Terminal Tickets

The system SHALL issue short-lived, single-use terminal tickets only
to authenticated sessions holding Owner or Admin role over the target
site. A ticket SHALL expire after its TTL (default 30 s), SHALL be
consumed by exactly one WebSocket upgrade, and SHALL be rejected when
replayed, expired, or unknown.

#### Scenario: Ticket consumed once

- **WHEN** a valid ticket completes a WebSocket upgrade and the same
        ticket is presented again
- **THEN** the second upgrade is closed with code 1008.

#### Scenario: Expired ticket rejected

- **WHEN** a ticket older than its TTL is presented
- **THEN** the upgrade is refused with `TerminalError::Expired`.

#### Scenario: Insufficient role

- **WHEN** a session without Owner/Admin over the site requests a
        ticket
- **THEN** the response is 403 without revealing whether the site
          exists.

### Requirement: Scoped PTY Session

The terminal SHALL spawn the PTY as the target site's jailed user with
working directory inside that site's root and a minimal environment;
it SHALL NEVER run as root. Output is bridged to the WebSocket with a
bounded buffer; uncontrolled output growth SHALL close the session.

#### Scenario: Runs as site user

- **WHEN** a user executes `id` in an opened terminal for site `s1`
- **THEN** the output identifies `s1`'s jailed user, not root.

#### Scenario: Overflow closes

- **WHEN** PTY output exceeds the configured buffer without reader
        progress
- **THEN** the session closes and audit records reason `overflow`.

### Requirement: Session Guardrails

Terminal sessions SHALL enforce an idle timeout, a per-user
concurrent-session cap, and an Origin check on the WebSocket upgrade;
the capability SHALL be feature-flagged (default off) and disabled
entirely for suspended accounts.

#### Scenario: Idle timeout reaps

- **WHEN** no input or output occurs for `idle_timeout_secs`
- **THEN** the server closes the session and audits closure.

#### Scenario: Origin mismatch rejected

- **WHEN** the upgrade request carries an Origin header that does not
        match the panel host
- **THEN** the upgrade is rejected before ticket consumption.

#### Scenario: Feature disabled

- **WHEN** `[web_terminal] enabled = false`
- **THEN** both ticket and session routes return 404.

### Requirement: Terminal Audit Without Content

Opening and closing a terminal SHALL emit audit events containing
user, site, timestamps, duration, and closure reason; keystrokes,
output bytes, and any derived content SHALL never be logged or
persisted.

#### Scenario: Content never recorded

- **WHEN** arbitrary data flows through any terminal session
- **THEN** no log line, audit event, or database row contains those
          bytes.
