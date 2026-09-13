# Proposal: Unify capability navigation and site workspaces

## Why

The shell has capability filtering and the site workspace has shared tabs, but
the router, capability inventory, navigation, and tab definitions are separate
lists. This causes mounted workflows to be undiscoverable and causes site tabs
for Domains, Runtime, Logs, and Backups to be intentionally absent.

## What Changes

- Introduce one route/capability metadata registry.
- Derive navigation, capability checks, breadcrumbs, and site tabs from it.
- Add missing site-scoped Domains, Runtime, Logs, and Backups entry points.
- Ensure unavailable, unauthorized, and not-yet-enabled states are distinct.
- Add route reachability and role-scope checks for every registered workflow.

## Capabilities

### Modified Capabilities

- `web-ui-discoverability`
- `panel-navigation`
- `site-workspace`
- `web-ui`

## Non-goals

- No visual redesign of every page.
- No permission model rewrite.
- No exposing capabilities whose backend module is not mounted.

## Dependencies

Depends on the quality ratchet. Browser UI quality can consume the registry
after this change lands.
