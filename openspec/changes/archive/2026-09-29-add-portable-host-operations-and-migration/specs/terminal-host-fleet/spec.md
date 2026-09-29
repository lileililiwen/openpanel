# terminal-host-fleet Specification

## Requirements

## ADDED Requirements

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
