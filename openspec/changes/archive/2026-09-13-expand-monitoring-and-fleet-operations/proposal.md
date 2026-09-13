# Proposal: Expand monitoring and fleet operations

## Why

OpenPanel has host gauges, history, synthetic monitoring, notifications, and
an mTLS agent model, but the operator experience does not yet match mature
panels with configurable dashboards, external uptime checks, threshold policy,
and fleet-level views.

## What Changes

- Add configurable metric panels, time ranges, refresh policy, and saved views.
- Add threshold policies with hysteresis and notification routing.
- Add website uptime checks independent of the panel process.
- Add fleet aggregation for agent status, version, health, and drift.
- Add stale/unknown/error semantics and bounded data retention.

## Capabilities

### Modified Capabilities

- `monitoring`
- `synthetic-monitoring`
- `terminal-host-fleet`
- `observability-export`
- `notifications`

## Non-goals

- No hosted observability SaaS.
- No unbounded time-series database in the panel process.
- No remote command execution in this change.

## Dependencies

Depends on quality maturity, release governance, and the capability registry.
