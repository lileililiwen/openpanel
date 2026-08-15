# Add Database Privilege Management

## Why

The `databases` capability (archived `add-databases-management`) covers
MySQL/MariaDB/PostgreSQL provisioning but does **not** cover per-user
grants, remote-access toggling, or web admin-tool integration. Today an
Owner gets a single all-privileged credential per database and no way to
scope a less-trusted application user, no way to allow a developer's
laptop to connect remotely, and no one-click phpMyAdmin / pgAdmin.
Panels such as cPanel, Plesk, CloudPanel, and 1Panel treat these as
table-stakes. This change extends the `databases` capability.

## What Changes

- Per-database user privilege management: `GRANT` scopes at db-level,
  table-level, and global level, exposed via
  `GET`/`PUT /databases/{id}/users/{uid}/privileges`.
- A remote-access toggle that manages the engine bind address and an
  access-control list (ACL) of allowed source CIDRs, via
  `PUT /databases/{id}/remote-access`.
- A one-click phpMyAdmin / pgAdmin single-sign-on link, via
  `/databases/{id}/admin-tool`, that mints a short-lived signed session
  (SSO) into the matching admin tool.
- New endpoints: `GET /databases/{id}/users/{uid}/privileges`,
  `PUT /databases/{id}/users/{uid}/privileges`,
  `PUT /databases/{id}/remote-access`,
  `POST /databases/{id}/admin-tool`.

## Capabilities

### Modified Capabilities

- `databases`: add per-user grant scopes (db/table/global), a
  remote-access toggle with an explicit ACL, and an SSO launch into
  phpMyAdmin / pgAdmin.

## Impact

- Domain: `DbGrant`, `GrantScope`, `RemoteAccess`, `RemoteAcl`,
  `AdminToolSession`.
- App: `PrivilegeService`, `RemoteAccessController`,
  `AdminToolSso`.
- API/CLI/web: `/databases/{id}/users/{uid}/privileges`,
  `/databases/{id}/remote-access`, `/databases/{id}/admin-tool`; CLI
  `openpanel db user {grants,remote,tool}`; web Database user editor.
- Security: every privilege and remote-access change is audited; the
  remote-access ACL SHALL NEVER default to `0.0.0.0/0` — a wildcard
  source requires an explicit owner opt-in flag; admin-tool SSO uses a
  short-lived, single-use signed token bound to the requesting user.
- Coupling: depends on the `databases` cap for credentials and engine
  control; depends on `identity` for SSO session signing and RBAC;
  interacts with `sites` for co-located app access patterns.

## Security

- Privilege changes are recorded in the audit log with the prior and
  resulting grant set (privilege names only, never passwords).
- The remote-access ACL never defaults to `0.0.0.0/0`; a wildcard
  source is only applied when the request carries an explicit
  `allow_any_source=true` opt-in from the Owner.
