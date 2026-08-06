## ADDED Requirements

### Requirement: Site Aggregate

The sites context SHALL model a `Site` aggregate with fields:

- `id` — UUID v4
- `owner_id` — UUID of the owning user (FK to identity.users)
- `primary_domain` — RFC-1035-compliant hostname (lowercase, no scheme)
- `aliases` — Vec<String> of additional hostnames serving the same content
- `document_root` — absolute filesystem path; default `/var/www/<primary_domain>/public_html`
- `php_enabled` — bool, default false (full PHP support lands in v0.2)
- `php_version` — optional enum, present only when `php_enabled`
- `status` — `active` or `disabled`
- `created_at`, `updated_at` — timestamps
- `audit_meta` — created_by, last_modified_by

`Site::new(...)` MUST validate the primary domain, reject duplicate
aliases, and reject document roots outside `/var/www/` unless an explicit
allowlist path is configured.

#### Scenario: Create a site

- **WHEN** the sites service receives `create_site(owner_id="...",
  primary_domain="example.com", aliases=["www.example.com"],
  php_enabled=false, document_root=None)`
- **THEN** it persists a `Site` with `status=active`, default document
  root `/var/www/example.com/public_html`, and `created_at = now()`.

#### Scenario: Reject invalid domain

- **WHEN** the operator submits `primary_domain="not a domain"`
- **THEN** the service returns `SiteError::InvalidDomain` and persists
  nothing.

### Requirement: nginx Config Generation

The sites context SHALL generate a valid nginx server block for each
`Site` and write it to
`/etc/nginx/conf.d/openpanel/<primary_domain>.conf`. When the site is
disabled, the config file SHALL be moved to
`/etc/nginx/conf.d/openpanel/disabled/<primary_domain>.conf.disabled`.

The generator MUST run `nginx -t` after writing; if the test fails, the
service MUST roll back the file change and return `SiteError::NginxTest`.

#### Scenario: Active site has live config

- **WHEN** a site is `active`
- **THEN** a valid nginx server block exists at
  `/etc/nginx/conf.d/openpanel/<primary_domain>.conf` with `server_name`,
  `root`, `index`, and an `access_log` / `error_log` directive.

#### Scenario: Disabled site has disabled config

- **WHEN** a site is transitioned to `disabled`
- **THEN** its config file is moved under `disabled/` with a `.disabled`
  suffix and nginx reload is run.

### Requirement: nginx Reload

After any config write, move, or delete, the sites context SHALL run
`nginx -t && nginx -s reload`. The service MUST capture stderr; if
`nginx -t` exits non-zero, the prior state MUST be restored and the
operation MUST fail with `SiteError::NginxTest`.

#### Scenario: Successful reload

- **WHEN** a site is created with valid config
- **THEN** `nginx -t` exits 0 and `nginx -s reload` exits 0; the new
  vhost is live without downtime.

#### Scenario: Bad config rolls back

- **WHEN** the generated nginx config has a syntax error
- **THEN** the previous config file is restored and
  `SiteError::NginxTest("...")` is returned with the captured stderr.

### Requirement: Document Root Provisioning

The sites context SHALL create the document root directory on
`create_site` if it does not exist, with mode `0755` and ownership of
the site's owner (resolved via `/etc/passwd` lookup of the owner's
username). The directory MUST be empty except for an auto-generated
`index.html` placeholder.

#### Scenario: Fresh document root

- **WHEN** a site is created and the document root does not exist
- **THEN** the directory is created with mode 0755 and a placeholder
  `index.html` is written that displays the site name and owner.

### Requirement: Per-Site RBAC

The sites context SHALL enforce the following RBAC rules:

- **Owner** (role) — sees and manages all sites.
- **Admin** (role) — sees all sites, can manage sites owned by themselves
  or by users with `User` role.
- **User** (role) — sees and manages only sites they own.

Service methods `list_sites(caller)`, `delete_site(caller, id)`,
`enable_site(caller, id)`, `disable_site(caller, id)` MUST accept a
`caller: &User` argument and reject with `SiteError::Forbidden` when
the caller lacks permission.

#### Scenario: User role limited to own sites

- **WHEN** a User calls `list_sites`
- **THEN** the response contains only sites where `owner_id == caller.id`.

#### Scenario: Admin manages User-owned sites

- **WHEN** an Admin calls `delete_site(caller, user_owned_site_id)`
- **THEN** the operation succeeds and an audit event `site_deleted` is
  recorded.

#### Scenario: Admin cannot manage Owner-owned sites

- **WHEN** an Admin calls `delete_site(caller, owner_owned_site_id)`
- **THEN** the service returns `SiteError::Forbidden` and records
  `permission_denied` audit event.

### Requirement: Site Lifecycle CLI

The CLI SHALL expose `openpanel site {create,list,delete,enable,disable}`
using the same service. The CLI is the canonical way to script vhost
provisioning.

#### Scenario: CLI creates site

- **WHEN** the operator runs `openpanel site create --domain example.com
  --owner admin --aliases www.example.com --php off`
- **THEN** the site is created with the given parameters, the operator's
  session is used as `created_by`, and the new site's id is printed.

### Requirement: Site HTTP Routes

The HTTP API SHALL expose:

- `POST /api/v1/sites` — create (admin or owner role required)
- `GET /api/v1/sites` — list (scoped to caller)
- `GET /api/v1/sites/{id}` — fetch one
- `DELETE /api/v1/sites/{id}` — delete
- `POST /api/v1/sites/{id}/enable` — set active
- `POST /api/v1/sites/{id}/disable` — set disabled
- `PATCH /api/v1/sites/{id}` — change owner or aliases

#### Scenario: List as User returns own sites

- **WHEN** a User calls `GET /api/v1/sites`
- **THEN** the response contains only sites owned by that user.

#### Scenario: Non-admin cannot create

- **WHEN** a User calls `POST /api/v1/sites`
- **THEN** the API returns 403 with code `forbidden`.

### Requirement: Audit Trail for Site Mutations

Every site lifecycle event (`site_created`, `site_deleted`,
`site_enabled`, `site_disabled`, `site_owner_changed`) SHALL be
recorded in `audit_log` via the architecture-owned `AuditService`.

#### Scenario: Create records audit event

- **WHEN** a site is created
- **THEN** an audit row is appended with `action='site_created'`,
  `actor=<caller username>`, `target=<site id>`.