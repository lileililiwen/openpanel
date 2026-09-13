# Design: Unify capability navigation and site workspaces

## Approach

Create a typed metadata inventory at the web composition boundary. Each entry
declares capability key, route, label key, icon, minimum role, scope, and
optional site-tab identity. Router mounting remains explicit, but startup and
tests validate that every discoverable entry is mounted and every mounted
first-class page has metadata.

## Explore & Reuse

- Reuse `CapabilitySet`, `NAV_SECTIONS`, `NavItem`, `workspace_tabs`, and
  `WebRuntime` capability composition.
- Reuse existing role extractors, route modules, breadcrumbs, and UI states.
- Reuse current navigation and site-workspace integration tests as the
  contract boundary.

## Boundaries

The registry is metadata only; authorization remains in handlers/services.
Site tabs are generated from registered site-scoped routes, never from an
independent hardcoded list. Missing backend routes cannot be represented as
active links.

## Verification

Test uniqueness, icon/label presence, route mounting, role filtering, direct
access protection, site context persistence, and unavailable-state behavior.

## Non-goals

- Replacing Axum routing.
- Adding a frontend router.
- Making every admin operation site-scoped.
