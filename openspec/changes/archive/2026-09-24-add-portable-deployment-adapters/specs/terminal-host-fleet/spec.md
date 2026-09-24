# terminal-host-fleet Specification

## Requirements

## ADDED Requirements

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
