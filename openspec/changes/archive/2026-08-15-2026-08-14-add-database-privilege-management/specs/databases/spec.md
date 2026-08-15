## ADDED Requirements

### Requirement: Per-User Privilege Management

The system SHALL let an authorised caller manage grants for a database
user at `Global`, `Database`, or `Table` scope via
`GET`/`PUT /databases/{id}/users/{uid}/privileges`. A grant request
SHALL be rejected if a `Table` scope names a table that does not belong
to the database, and a `Global` scope SHALL require an Owner.

#### Scenario: Apply scoped grants

- **WHEN** an Owner `PUT`s db-level and table-level grants for user
        `u1` on `db1`
- **THEN** the engine reflects the `GRANT`s and audit
        `DbPrivilegesChanged{uid, before, after}` records privilege
        names only (never passwords).

#### Scenario: Cross-database grant rejected

- **WHEN** a request scopes a grant to a table outside `db1`
- **THEN** the request is rejected with
        `DbPrivilegeError::ScopeNotInDatabase`.

### Requirement: Remote-Access Toggle

`PUT /databases/{id}/remote-access` SHALL enable or disable remote
connectivity by setting the engine bind address and an ACL of allowed
source CIDRs. The ACL SHALL NEVER default to `0.0.0.0/0`; a wildcard
source is applied only when the request carries an explicit
`allow_any_source=true` owner opt-in.

#### Scenario: Enable with explicit CIDR

- **WHEN** an Owner enables remote access for `db1` with `acl=[10.0.0.0/8]`
- **THEN** the engine binds and permits only that range; audit
        `RemoteAccessChanged{db_id, acl}`.

#### Scenario: Open-by-default rejected

- **WHEN** an Owner enables remote access with an empty ACL and no
        `allow_any_source`
- **THEN** the request is rejected with
        `RemoteAccessError::NoOpenByDefault`, and the engine bind is
        unchanged.

### Requirement: Admin-Tool Single Sign-On

`POST /databases/{id}/admin-tool` SHALL mint a short-lived, single-use
signed SSO session into phpMyAdmin or pgAdmin for the requesting user,
returning a redirect that authenticates them without re-entering
database credentials.

#### Scenario: Launch phpMyAdmin

- **WHEN** an Owner launches the admin tool for `db1` with `tool=PhpMyAdmin`
- **THEN** a single-use signed token is issued, the tool opens an
        authenticated session for `db1`, and audit
        `AdminToolLaunched{db_id, tool}` is recorded.

#### Scenario: Expired token refused

- **WHEN** a previously used or expired SSO token is replayed
- **THEN** the tool returns `401` and no session is created.
