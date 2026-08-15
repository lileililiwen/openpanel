# Add scheduled maintenance windows

## Why

Operators sometimes need a maintenance window during which
destructive mutations are blocked (e.g. "no installations
during peak hours"). The `cron`, `backups`, and
`software-center` capabilities all emit such mutations. cPanel
has no formal freeze mechanic; Baota allows cron-driven
freeze. This change adds a typed `MaintenanceWindow` that
gates destructive operations across the panel, with a typed
override path for break-glass scenarios.

## What Changes

- New bounded context `maintenance-windows` carrying
  `MaintenanceWindow` aggregate and `MaintenanceEnforcer`.
- New endpoints: `GET/POST/PUT/DELETE /admin/maintenance`,
  `POST /admin/maintenance/override`.
- The enforcer is a middleware-like guard consulted by every
  bounded context before any destructive action.
- New SQLite tables: `maintenance_windows`,
  `maintenance_overrides` (audit-only).

## Capabilities

### New Capabilities

- `maintenance-windows`: scheduled freeze of destructive
  operations with typed override.

## Impact

- Domain: `MaintenanceWindow`, `MaintenanceOverride`,
  `DestructiveActionClass`.
- App: `MaintenanceEnforcer`, integration with `cron`,
      `software-center`, `backups`, `containers`,
      `malware-scanner`, `web-application-installer`, etc.
- API/CLI/web: `/admin/maintenance/*`; CLI
  `openpanel admin maintenance {get,set,clear,override}`; web
  admin tab.
- Coupling: integrates with notification channels so users
  are warned before a window starts.
