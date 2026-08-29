# Repair UI discoverability and capability registration

## Why

OpenPanel contains many web routes, but `CapabilitySet::shipped()` exposes only a small subset. Mail, DNS, cron, services, backups, logs, security, software, Docker, webmail, and related workflows can therefore be absent from the sidebar. The status-page item also references an unregistered `pulse` icon; the existing web crate test fails on this defect.

## What

Make navigation derive from the same registered capability inventory as the web router. Add a stable route-to-capability contract, role-aware visibility, active-state matching for nested routes, icon validation, and a discoverability test that fails when a shipped page is unreachable from navigation.

## Capabilities

### New

- Complete capability registration for all shipped web modules.
- Navigation contract tests for owner and user roles.
- Built-in icon coverage for every navigation item.

### Modified

- Sidebar grouping and labels may change to reflect user tasks rather than implementation modules.
- Unavailable or owner-only features remain hidden and direct URLs remain protected.

## Non-goals

- No new backend module.
- No visual redesign beyond navigation clarity.
- No implementation of currently stubbed pages.

## Dependencies

None. This change should be implemented first.
