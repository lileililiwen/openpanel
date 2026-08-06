# Add Sites Management

## Why

OpenPanel v0.1-alpha ships identity + auth so the operator can log in. To
manage websites, the operator next needs to provision a vhost — point a
domain at a document root, generate an nginx server block, and reload
nginx. Without this, the panel has no value beyond user management.

This change adds the `sites` bounded context as the first feature built on
the DDD architecture that was bootstrapped in
`bootstrap-ddd-architecture`.

## What Changes

- New `Site` aggregate in `openpanel-domain/sites/` (domain): id, owner,
  primary domain, list of aliases, document root, PHP toggle, status
  (`active` / `disabled`), TLS toggle (placeholder for ssl module), audit
  fields.
- New `SiteRepository` trait in domain; SQLite adapter in
  `openpanel-app/sites/repo.rs`.
- New `SitesService` in `openpanel-app/sites/service.rs`: `create_site`,
  `list_sites`, `delete_site`, `enable_site`, `disable_site`,
  `change_owner`, `find_by_domain`. Each mutation calls `audit.record`.
- New nginx config generator in `openpanel-app/sites/nginx.rs` that
  emits `/etc/nginx/conf.d/openpanel/<domain>.conf` from a template
  and reloads nginx via `nginx -t && nginx -s reload`.
- New `SitesModule` (impls `openpanel_core::Module`) wiring service,
  migrations, and HTTP routes.
- New HTTP routes under `/api/v1/sites/*` in `openpanel-api/src/routes/sites.rs`:
  - `POST /sites` create site (owner or site-owning admin)
  - `GET /sites` list sites (scoped to caller's role)
  - `GET /sites/{id}` fetch one
  - `DELETE /sites/{id}` remove vhost + nginx config
  - `POST /sites/{id}/enable` and `/disable`
  - `PATCH /sites/{id}` change owner / aliases
- New CLI subcommand `openpanel site {create,list,delete,enable,disable}`.
- New migration `sites/V001__init.sql` defining the `sites` table.

## Capabilities

### New Capabilities

- `sites` — site (vhost) provisioning, nginx config generation, lifecycle
  management (create / list / delete / enable / disable), per-site RBAC.

### Modified Capabilities

- `architecture` — Module trait now also used by `SitesModule`;
  `openpanel-app/hosting/` directory will become the canonical location
  for site/SSL/database/file submodules (currently empty placeholder).
- _No behavioral changes_ to existing requirements.

## Impact

- `crates/openpanel-domain/src/sites/` — new module (Site, SiteStatus,
  SiteDomain, SiteError, SiteRepository trait)
- `crates/openpanel-app/src/sites/` — new module (SitesService, repo,
  SitesModule, nginx.rs)
- `crates/openpanel-app/src/migrations/sites/V001__init.sql` — new
- `crates/openpanel-api/src/routes/sites.rs` — new
- `crates/openpanel-api/src/router.rs` — nest sites under `/api/v1`
- `crates/openpanel-cli/src/commands.rs` + `handlers.rs` — `site`
  subcommand
- `Cargo.toml` — `notify` is already a workspace dep (file watcher); the
  nginx template uses it for hot-reload on dev machines (future use)
- All existing crates unchanged otherwise