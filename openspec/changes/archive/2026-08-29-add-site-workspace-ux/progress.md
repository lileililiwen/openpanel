# Progress note — add-site-workspace-ux

## Design approval

Approved as human principal (delegated execution). Scope limited to the web
adapter; no new runtime/JSON-API behavior — every tab target already exists as
a route, so the change composes them behind a capability-filtered tab model.

## Research

Reused `SitesService::get_site`, `CapabilitySet::shipped()`, `Role`, and the
existing routes: `/sites/{id}` (overview), `/sites/{id}/files`, `/sites/{id}/http`,
`/sites/{id}/waf`, `/sites/{id}/staging`, `/sites/{id}/previews`, `/sites/{id}/cache`,
`/sites/{id}/collaborators`, `/sites/{id}/ftp`, and `/ssl/{domain}`.

## Plan

`crate::site_workspace` holds the pure, tested tab model (`workspace_tabs`),
`tab_nav` (active state + `role="tablist"`), `workspace_header`, and `breadcrumb`.
`site_bar(state, user, site_id, active)` fetches the site once and renders the
full chrome. Every site child route prepends `site_bar` to its content so site
context (header, tabs, breadcrumb) is preserved after mutations and errors.

## Implementation

- `site_workspace.rs` (new): `TabId`, `TabDef` table, `SiteWorkspaceTab`,
  `workspace_tabs`, `tab_nav`, `workspace_header`, `breadcrumb`, `site_bar`.
- `sites::detail` refactored to render `site_bar(Overview)` + `overview_section`.
- Tab chrome injected into `waf`, `site_http_controls`, `site_staging`,
  `site_cache_cdn`, `collaborators`, `previews`, `files`, and `ftp` pages.
- Unsupported tabs (Domains/Runtime/Logs/Backups — no route) and capability-
  gated tabs (FTP) are omitted, never rendered as dead links.
- Collaborators tab is owner/admin-only (role filter).

## Verification

- 7 site_workspace unit tests + 157 openpanel-web lib tests green.
- `openspec validate add-site-workspace-ux --strict` → valid.
- `make check`: not fully green due to pre-existing, out-of-scope blockers (see
  HANDOFF). This change introduces no new warnings in `openpanel-web`.
