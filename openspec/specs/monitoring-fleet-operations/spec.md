# monitoring-fleet-operations Specification

## Purpose
TBD - created by archiving change expand-monitoring-and-fleet-operations. Update Purpose after archive.
## Requirements
### Requirement: Monitoring Views Are Configurable

Operators MUST be able to select bounded metric panels, time ranges, refresh
policy, and saved layouts without changing collection logic.

#### Scenario: Custom time range

- **WHEN** an operator selects a supported historical range
- **THEN** charts query that range and clearly show empty, stale, or unavailable data.

### Requirement: Thresholds Have Hysteresis

Threshold policies MUST avoid alert flapping, emit one transition event per
state change, and route notifications through the existing dispatcher.

#### Scenario: Recovery after breach

- **WHEN** a metric returns below its recovery threshold after a breach
- **THEN** one recovery event is emitted and repeated samples do not duplicate it.

### Requirement: Uptime Monitoring Is Independent

Website availability probes MUST execute independently of the panel process,
record bounded results, and distinguish panel-down from target-down states.

#### Scenario: Panel unavailable

- **WHEN** the panel process is down but an external probe remains scheduled
- **THEN** the target result remains observable through the independent probe path.

### Requirement: Fleet Health Is Safe and Scoped

Fleet views MUST show heartbeat freshness, version/configuration drift, and
health by authorized host scope without exposing certificates, tokens, or
command material.

#### Scenario: Expired heartbeat

- **WHEN** an agent misses its heartbeat deadline
- **THEN** the host is marked stale with last-seen time and recovery guidance.

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

