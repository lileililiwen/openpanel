# monitoring-fleet-operations Specification

## Requirements

## ADDED Requirements

### Requirement: Fleet Health Surfaces Host Capability State

Fleet health views MUST be able to observe each managed host's capability
profile and health state so an operator can tell a healthy host from one whose
declared runtime adapters are unavailable. The view MUST reuse the canonical
`openpanel_core::audit::redact_metadata` allowlist and MUST NOT surface the
capability profile's credential-bearing material.

#### Scenario: Capability health appears in the fleet view

- **WHEN** a host reports a degraded capability profile
- **THEN** the fleet view names the host, the degraded adapter or failed probe,
  and the panel version, without exposing any credential or command material.
