## Context

`NAV_LINKS` is currently a static flat list. OpenPanel has seven shipped pages and eight proposed modules, so continuing to append links will create a crowded menu and expose routes that some roles cannot use.

## Goals / Non-Goals

**Goals:** provide stable information architecture, derive visibility from registered capabilities and RBAC, work on narrow screens, and expose a small safe settings page.

**Non-Goals:** user-customized menu ordering, arbitrary configuration-file editing, a marketplace, or placeholder links for unimplemented modules.

## Decisions

1. Replace tuples with typed `NavSection` and `NavItem` descriptors containing route, label, capability, and minimum authorization. The router composition root supplies the enabled capability set; this avoids duplicating module state in templates.
2. Use five groups: Overview; Hosting (Sites, Files, Databases, SSL); Operations (Monitoring, Logs, Backups, Cron, Services); Security & Network (Security, DNS, Mail); Administration (Users, Settings). Empty groups are not rendered.
3. Determine the active item from the request URI and render semantic `aria-current`; render breadcrumbs from the same descriptors so labels cannot drift.
4. Keep the shell server-rendered. A CSS disclosure control provides mobile navigation; no new frontend dependency is introduced.
5. `/settings` exposes installation version, data/config paths with secrets redacted, and an allowlist of mutable preferences (theme, locale, timezone). Writes use CSRF, validate through the existing configuration schema, persist atomically, and audit the changed field names only.

## Risks / Trade-offs

- Capability discovery can drift from router registration -> derive both from the same composition data and test every registered module.
- Configuration paths can reveal sensitive topology -> restrict details to Owner and redact credentials and keys.
- A navigation refactor can break bookmarks -> preserve all existing routes and test redirects/links.

## Migration Plan

Deploy as a template/config migration with no database migration. Rollback restores the old shell; stored preferences remain harmless configuration keys.
