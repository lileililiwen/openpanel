## Why

OpenPanel's flat seven-link sidebar matches the implemented alpha modules but will not scale to the operational areas expected of a server panel. The navigation and settings surface must become capability-aware before new modules are added, without advertising unavailable features.

## What Changes

- Group navigation into Overview, Hosting, Operations, Security & Network, and Administration.
- Show links only for registered capabilities and actions allowed by the caller's role.
- Add active states, a mobile navigation control, breadcrumbs, and consistent page titles.
- Add `/settings` for safe panel preferences and read-only installation information.
- Keep existing URLs stable.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `web-ui`: make the shell scalable, responsive, role-aware, and add panel settings.

## Impact

Changes are limited to `openpanel-web`, the web composition root, configuration read models, and web integration tests. No domain module or public API route is removed.
