# collaborators Specification

## Purpose
TBD - created by archiving change 2026-08-14-add-per-site-collaborator-permissions. Update Purpose after archive.
## Requirements
### Requirement: Invite Collaborator

The system SHALL let an account invite a collaborator account scoped to
a specific site with a limited permission set drawn from file, database,
mail, and cron. The collaborator SHALL belong to the inviting account and
SHALL be separate from global account roles. Each invite SHALL be
audited.

#### Scenario: Invite with scoped permissions

- **WHEN** an account owner invites `dev@example.com` to site `s1` with
        file + database scope
- **THEN** a `Collaborator` (status `Invited`) and a `SiteGrant` for
        `s1` exist, and audit `CollaboratorInvited{site, scopes}` records
        the scopes only.

#### Scenario: Scope limited to allowed set

- **WHEN** an invite requests a scope outside file/database/mail/cron
- **THEN** the request is rejected and no grant is created.

### Requirement: Resolve Scoped Access

The system SHALL resolve a collaborator's effective access for a site as
the union of that site's grants. The effective access SHALL NOT include
the account-level role, and SHALL gate file, database, mail, and cron
operations accordingly.

#### Scenario: Grant allows scope

- **WHEN** a collaborator with database scope acts on site `s1`'s database
- **THEN** the action is permitted.

#### Scenario: Grant denies scope

- **WHEN** the same collaborator acts on site `s1`'s mail
- **THEN** the action is denied because mail is outside the grant.

### Requirement: Revoke Collaborator

The system SHALL let an account revoke a collaborator from a site,
removing the `SiteGrant` and all effective access for that site, and
SHALL audit `CollaboratorRevoked`. Revocation SHALL NOT alter global
account roles.

#### Scenario: Revoke removes access

- **WHEN** a collaborator is revoked from site `s1`
- **THEN** the `SiteGrant` is removed, subsequent access is denied, and
        the account role is unchanged.

