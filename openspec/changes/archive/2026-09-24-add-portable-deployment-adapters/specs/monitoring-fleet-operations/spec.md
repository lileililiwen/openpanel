# monitoring-fleet-operations Specification

## Requirements

## ADDED Requirements

### Requirement: Fleet Operations Compose With Deployment Adapters

Fleet operations (heartbeat, version drift, configuration drift, host scope)
MUST be observable alongside deployment-adapter runs so an operator can
correlate a host's health with the adapter that last acted on it. The
deployment-adapter service records a typed `SoftwareChanged` audit event per
mutating run with a stable evidence id; the fleet view MAY join on that id
without duplicating audit material. The fleet scope MUST NOT surface the
adapter's host path, transport command, or secret value — the
`deployment-adapters` contract already redacts those, and the fleet view
MUST reuse the canonical `openpanel_core::audit::redact_metadata` allowlist
instead of re-implementing it.

#### Scenario: Adapter run appears in fleet audit trail

- **WHEN** a deployment adapter completes a `Deploy` action on a host
- **THEN** the resulting `SoftwareChanged` audit event is visible to a
  fleet view scoped to that host and the event's metadata contains the
  adapter id, the action name, the release digest, the evidence id, and
  the redacted state — but no host path, no transport command, and no
  secret value.
