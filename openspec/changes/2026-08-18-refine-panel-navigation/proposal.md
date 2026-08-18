# Refine panel navigation and UX

## Why

OpenPanel is a Baota / aaPanel-class server panel, but its sidebar
has drifted into a flat, capability-shaped list: **24 items across
five sections**, no icons, no collapsible groups, no search, and no
visual hierarchy. aaPanel and Baota both organize the same feature
set into a compact, user-centered sidebar — a dashboard entry, then
per-workload groups (Websites, Databases, Mail, Files), an operations
group (Monitor, Cron, Logs, Backups), and a system/apps group — each
with icons, collapsible subgroups, and a filter box. Operators
switching from those panels expect that model; our flat list makes
Owner-only administration items indistinguishable from daily-driver
items and forces scrolling through 24 entries.

This change refactors the `openpanel-web` Shell navigation into that
grouped, icon-augmented model while keeping the existing
capability/role gating exactly as it works today.

## What Changes

- Reorganize the sidebar into six user-centered sections: Overview,
  Websites, Mail & Network, Operations, Apps, System.
- Add a shared inline-SVG icon set rendered next to every nav item.
- Make each section collapsible via a native disclosure, with the
  open/closed state persisted per user.
- Add a sidebar filter box that narrows visible items as the user
  types.
- Add a compact icon-only "rail" mode for wide screens.
- Preserve capability/role filtering, breadcrumbs, the mobile
  disclosure, and the responsive styling contract.

## Capabilities

### New Capabilities

- `panel-navigation`: grouped, icon-augmented sidebar navigation with
  collapsible sections, search filter, and compact rail mode.

## Impact

- Web: `openpanel-web/src/nav_model.rs` (new), `layout.rs` rework,
  `assets.rs` CSS tokens for rail/search/disclosure. No API, domain,
  or app changes.
- No route, capability, or role-gating semantics change.

## Non-goals

- No changes to routes, capabilities, or role-gating semantics.
- No per-page content redesign beyond the sidebar.
- No third-party icon library or JS framework; icons are inline SVG,
  and the only client JS is a small vanilla script for collapse
  persistence and the filter box.
- Badges with live counts are deferred; the nav ships static presence
  only.
- The audit page remains a stub; no page is added or removed.
