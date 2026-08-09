# Add Web UI — Sites

## Why

Site provisioning is the core of a hosting panel, and the JSON API + CLI
already implement it fully (`create/list/get/delete/enable/disable`,
nginx vhost provisioning, document root creation). The dashboard card now
links nowhere. This change gives the operator the sites management pages in
the web shell — the same flows the CLI exposes, through forms and HTMX.

## What Changes

- `crates/openpanel-web/src/sites.rs`:
  - `GET /sites` — site list inside the shell, one row per site (domain,
    status, owner, php), with detail/enable/disable/delete actions.
  - `GET /sites/new` — create form (domain, aliases, owner, PHP toggle +
    version, document root override).
  - `POST /sites` — create; renders the list (HTMX swap) or shows a
    validation/duplicate-domain error.
  - `GET /sites/{id}` — site detail: domain, aliases, document root, PHP
    settings, status, quick links (files, ssl).
  - `POST /sites/{id}/enable` and `/sites/{id}/disable` — status toggles.
  - `DELETE /sites/{id}` — delete with confirmation.
- All handlers run through the foundation shell, `WebUser` gating, and CSRF
  validation.

## Non-Goals

- Editing aliases/owner in the UI (service supports it; UI edit form is a
  follow-up).
- Per-site traffic stats or logs (separate capability, not yet built).
- Bulk operations.

## Capabilities

### Existing Capabilities

- `web-ui`: adds the sites pages to the shell.
- `sites`: consumed through the `SitesService`.
