# Add Per-site collaborator permissions

## Why

`account-hierarchy` (active) models account-level organisation, but
there is **no per-site sub-account with scoped permissions** — the
Plesk "additional users" / cPanel subaccounts pattern. Today a site
owner must share a full account credential to let a contractor touch one
site's files, database, or mail. This change adds a `collaborators`
bounded context: collaborator accounts scoped to specific sites with
limited permission sets (file / database / mail / cron), kept separate
from the global account roles.

## What Changes

- New bounded context `collaborators` carrying the `Collaborator`,
  `SiteGrant`, and `PermissionSet` aggregates, plus
  `CollaboratorService`, `GrantResolver`.
- New endpoints: `POST /api/v1/sites/{id}/collaborators`,
  `GET/PUT/DELETE /api/v1/sites/{id}/collaborators/{uid}`.
- Invite a collaborator account scoped to one or more sites; revoke at
  any time.
- A permission set limited to file / database / mail / cron scopes —
  distinct from the global roles in `identity` and the account hierarchy.
- Grant resolution: a collaborator's effective access is the union of
  their site grants, never the account-level role.

## Capabilities

### New Capabilities

- `collaborators`: invite collaborator accounts scoped to specific sites
  with limited permission sets (file/db/mail/cron), separated from
  global roles.

## Impact

- Domain: `Collaborator`, `SiteGrant`, `PermissionSet`.
- App: `CollaboratorService`, `GrantResolver`.
- API/CLI/web: `/sites/{id}/collaborators`, `/sites/{id}/collaborators/{uid}`;
  web Collaborators tab (CSRF).
- Security: collaborator access is scoped per site and per scope; grants
  are revocable and audited; a collaborator never inherits account roles.
- Coupling: depends on `identity` for authentication/principal and on
  `account-hierarchy` for the account boundary; operates per site via
  `sites`.
