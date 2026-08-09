## Why

Sites, databases, and TLS depend on nginx, MySQL, and PHP-FPM, yet OpenPanel cannot report or recover those services. Service health and controlled lifecycle actions are a smaller and safer foundation than an app store or arbitrary package installer.

## What Changes

- Add allowlisted service discovery, status, start/stop/restart/reload, enablement, and health monitoring.
- Surface recent journal entries and dependency impact before disruptive actions.
- Add REST, CLI, web UI, background checks, alerts, and audit.
- Do not install packages or manage arbitrary systemd units.

## Capabilities

### New Capabilities

- `system-services`: safe lifecycle and health management for OpenPanel dependencies.

### Modified Capabilities

None.

## Impact

Adds domain/app/API/CLI/web modules, a systemd adapter, health persistence, monitoring integration, and privileged action policy.
