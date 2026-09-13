# Design: Expand monitoring and fleet operations

## Approach

Keep current snapshot/history services and add a normalized metric-query and
policy layer. Saved dashboard views store bounded configuration, not metric
data. Synthetic probes run through supervised background tasks and publish
health events to the existing notification system. Fleet aggregation reads
agent heartbeats/manifests and never grants command authority.

## Explore & Reuse

- Reuse `MonitoringService`, metric samples, sparkline rendering, and existing
  dashboard widgets.
- Reuse synthetic-monitoring probe/status services and notification dispatcher.
- Reuse `AgentService`, mTLS identity, manifest validation, and host-fleet
  redaction models.
- Reuse existing retention, audit, role, and UI-state patterns.

## Boundaries

Metric collection, policy evaluation, notifications, and fleet inventory stay
separate services. The web/API/CLI adapters query projections; they do not
perform direct host probes or agent commands.

## Verification

Test time-window queries, saved views, threshold hysteresis, probe failures,
stale data, fleet heartbeat/version drift, authorization, retention, and
notification deduplication.

## Non-goals

- Remote shell or remediation.
- Unlimited historical storage.
- External SaaS account provisioning.
