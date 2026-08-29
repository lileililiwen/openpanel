# Add terminal and host-fleet UX

## Why

aaPanel supports local and remote SSH terminal workflows, while BaoTa documents node management, remote service state, logs, jobs, databases, and terminal access. OpenPanel has agent, SSH-key, web-terminal, and host-security domains, but they are not presented as one safe operator workflow.

## What

Add a secure terminal surface and host-fleet workspace with explicit host context, connection state, command safety, session lifecycle, service/log/job shortcuts, and auditable actions.

## Capabilities

### New

- Host list and host detail workspace.
- Browser terminal session lifecycle.
- Host-scoped links to services, logs, sites, databases, backups, and monitoring.

### Modified

- Existing web terminal, agent, SSH key, and security capabilities become discoverable and consistently scoped.

## Non-goals

- No arbitrary command allowlist bypass.
- No root credentials stored in browser state.
- No multi-host destructive bulk operations.

## Dependencies

Depends on `repair-ui-discoverability`; reuse existing web-terminal, agent, SSH-key, security, logs, and service-manager specs.
