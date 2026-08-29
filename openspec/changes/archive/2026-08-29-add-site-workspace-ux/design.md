# Design: Site workspace UX

## Explore & Reuse

- Reuse `sites::detail_section`, current site detail route, `site_http_controls`, `site_cache_cdn`, `site_staging`, `previews`, `ftp`, `waf`, `ssl`, `files`, and collaborator handlers.
- Reuse the shell breadcrumb renderer and existing `card`, `table`, `form`, `status`, and `ui_states` components.
- Do not duplicate resource authorization; each existing handler remains authoritative.

## URL and navigation

Use `/sites/{id}` as Overview and stable child paths such as `/sites/{id}/files`, `/sites/{id}/ssl`, `/sites/{id}/http`, `/sites/{id}/waf`, `/sites/{id}/staging`, and `/sites/{id}/previews`. Tabs are generated from capability and authorization checks. Unsupported tabs are omitted, not disabled dead links.

## UX rules

- Header shows domain, status, environment, runtime, SSL expiry, and last deployment.
- Primary actions are Create backup, Open files, Issue/renew SSL, and Preview/staging where available.
- Destructive actions use the existing confirmation layer.
- Site context is retained after mutations and errors.
- Mobile tabs become horizontally scrollable with visible focus and no hidden actions.

## Verification

Integration tests cover tab visibility, owner/user scope, nested active state, return paths, unsupported capability omission, and no cross-site access.
