## Context

OpenPanel shells out to nginx and MySQL but does not own service state. Arbitrary systemd access would create a root command surface, so targets and actions must come from configuration, not request text.

## Goals / Non-Goals

**Goals:** accurate status, allowlisted actions, dependency impact, health history, bounded logs, alerts, and audit.

**Non-Goals:** package installation/upgrades, arbitrary units, editing unit files, process supervision for user apps, containers, or browser terminal.

## Decisions

1. Register typed service descriptors at composition (`nginx`, `mysql`, configured PHP-FPM units) with fixed unit name, supported actions, dependencies, and health probe. Requests carry descriptor IDs only.
2. Use a `ServiceController` port backed by `systemctl show/start/stop/restart/reload/enable/disable` with fixed argv and timeouts. Unsupported init systems are read-only unavailable, not guessed.
3. Require Owner for stop/restart/enablement; Admin may view and reload where declared safe. Preview lists affected OpenPanel capabilities and active resources; stop/restart requires confirmation.
4. A supervised task checks active state and probes with hysteresis, records transitions, and emits monitoring/audit events. Auto-restart is disabled by default and rate-limited when enabled per descriptor.
5. Journal access uses fixed unit filters, cursor/size bounds, and the logs redaction model.

## Risks / Trade-offs

- Restart causes downtime -> impact preview, confirmation, readiness wait, and clear partial-failure state.
- Unit names vary -> explicit distro/config mapping and unsupported status.
- Restart loops -> opt-in, attempt budget, cooldown, and alert escalation.

## Migration Plan

Ship descriptors/status read-only first. Enable mutations only when controller capability checks pass; rollback leaves system services in their last operator-selected state.
