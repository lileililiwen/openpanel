# Redesign the operations dashboard

## Why

The current dashboard contains host gauges, five counts, and recent alerts. aaPanel presents server identity, operating system, CPU, memory, disk, network, website/database counts, security risks, and software status on the home page. OpenPanel's security and reliability strengths are not visible at first use.

## What

Create a role-aware operations dashboard with server identity, health summary, resource trends, service state, security posture, backup/job status, and contextual actions. Preserve server rendering and HTMX refreshes.

## Capabilities

### New

- Health summary cards with explicit status and last-updated times.
- Network and disk-capacity views.
- Security, backup, and job attention queues.
- Quick actions linked to existing workflows.

### Modified

- Existing gauges, counts, and alerts become dashboard widgets with loading/error states.
- Dashboard avoids presenting fallback zeros as healthy measurements.

## Non-goals

- No new monitoring backend or metrics protocol.
- No charting dependency unless existing SVG primitives cannot satisfy the design.
- No destructive action without existing confirmation flow.

## Dependencies

Depends on `repair-ui-discoverability`; audit links should integrate with `add-audit-activity-center` when available.
