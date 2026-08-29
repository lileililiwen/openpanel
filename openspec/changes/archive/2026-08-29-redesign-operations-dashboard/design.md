# Design: Operations dashboard

## Explore & Reuse

- Reuse `MonitoringService::snapshot_now`, history samples, alerts, service manager, backup status, and existing resource list services.
- Reuse dashboard `gauges_section`, `cards_section`, `alerts_section`, `ui_states`, SVG sparklines, and HTMX refresh patterns.
- Reuse semantic tokens and existing status classes; do not add a second color vocabulary.

## Layout

1. Page header: server name, OS/architecture, last refresh, refresh action.
2. Attention strip: security risks, failed backups/jobs, degraded services.
3. Resource grid: CPU, memory, disk capacity, load, network.
4. Resource links: sites, databases, files, SSL, mail.
5. Trend panels and recent activity.

Cards must include text status, not color alone. Every auto-refreshed region has `aria-live` or a busy state and displays the collection error instead of silently converting failure to zero.

## Role behavior

Users see resources they can access and personal/site-scoped activity. Owners see host, security, service, backup, and system controls. The server must never reveal counts or names outside the caller's scope.

## Verification

Test widget renderers with success, empty, stale, and failure data. Integration tests verify owner/user scope, responsive markup, refresh targets, accessible names, and links to existing routes.
