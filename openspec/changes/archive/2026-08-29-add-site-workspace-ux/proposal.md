# Add site workspace UX

## Why

OpenPanel currently distributes site operations across separate pages and links. aaPanel and BaoTa make the site the primary workspace, grouping domains, runtime, files, SSL, redirects, proxy, security, logs, backups, and deployment actions.

## What

Turn each site detail page into a consistent tabbed workspace. Add an overview with status and health, contextual navigation, safe quick actions, and clear scope for site-level resources.

## Capabilities

### New

- Site workspace navigation and overview.
- Tabs for domains, runtime, files, SSL, HTTP controls, WAF, logs, backups, staging, previews, FTP, and collaborators where supported.
- Site-scoped breadcrumbs and return paths.

### Modified

- Existing site detail links become workspace tabs.
- Existing capabilities remain the source of behavior; this change composes them.

## Non-goals

- No new runtime implementation.
- No replacement for existing per-module authorization.
- No cross-site bulk operations.

## Dependencies

Depends on `repair-ui-discoverability`. Coordinate with existing `site-http-controls`, staging, previews, cache/CDN, FTP, SSL, WAF, and collaborator specs.
