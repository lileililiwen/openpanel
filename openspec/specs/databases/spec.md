# databases Specification

## Purpose
TBD - created by archiving change add-databases-management. Update Purpose after archive.
## Requirements
### Requirement: Database Aggregate

The databases context SHALL model a `Database` aggregate with fields:

- `id` — UUID v4
- `owner_id` — UUID of the owning user (FK to identity.users)
- `name` — the MySQL database name; panel-enforced as
  `{owner_username}_{requested_name}` to guarantee uniqueness and
  ownership traceability. Lowercase, `[a-z0-9_]{3,32}`.
- `db_user` — the MySQL username; same format as `name`
- `db_host` — `localhost` for v0.1 (Unix socket auth)
- `engine` — `Mysql` for v0.1
- `charset` — `utf8mb4` default, configurable per request
- `status` — `Active` or `Suspended`
- `created_at`, `created_by`

The DB user's plaintext password is NOT stored on the aggregate; it
lives encrypted in the `password_ciphertext` column. The service
returns it only when explicitly requested via `reveal_password`.

#### Scenario: Create a database

- **WHEN** the service receives `create_database(owner_id="...",
  name="app", charset="utf8mb4")`
- **THEN** it allocates `db_name="{owner_username}_app"`,
  `db_user="{owner_username}_app"`, generates a 24-char random
  password, calls MySQL to CREATE DATABASE + CREATE USER + GRANT,
  persists the encrypted password, and returns the database record
  with the plaintext password in the response.

#### Scenario: Reject bad database name

- **WHEN** the operator submits `name="bad name!"`
- **THEN** the service returns `DatabaseError::InvalidName` and
  MySQL is NOT touched.

### Requirement: Password Encryption at Rest

The databases context SHALL encrypt each DB user's password with
AES-256-GCM using a master key supplied via config
(`database.master_key`, base64-encoded 32 bytes). Each record SHALL
have its own random 12-byte nonce. The nonce SHALL be stored alongside
the ciphertext in the SQLite table.

#### Scenario: Encrypt and roundtrip

- **WHEN** the service stores a password
- **THEN** the SQLite row contains `nonce || ciphertext` (concatenated,
  nonce first) and `reveal_password(id)` recovers the original
  plaintext.

#### Scenario: Missing master key

- **WHEN** `database.master_key` is not set
- **THEN** the databases module fails to start with
  `DatabaseError::MasterKeyMissing` and the panel exits non-zero.

### Requirement: MySQL Provisioning via Shell

The databases context SHALL use the `mysql` CLI to provision, modify,
and drop databases and users. All mutations MUST shell out:

- Create: `mysql -e "CREATE DATABASE ..."` then
  `mysql -e "CREATE USER ... IDENTIFIED BY '...'"`
  then `mysql -e "GRANT ALL ON db.* TO user@'localhost'"`.
- Change password: `mysql -e "ALTER USER ... IDENTIFIED BY '...'"`.
- Drop: `mysql -e "DROP DATABASE ..."` then
  `mysql -e "DROP USER ..."`.

The `mysql` binary MUST be detected at startup via `which mysql`;
if absent, the service returns `DatabaseError::MysqlMissing` on every
mutation and refuses to boot.

#### Scenario: MySQL missing fails gracefully

- **WHEN** `mysql` is not in PATH
- **THEN** the service refuses to create / drop databases, the
  `DatabasesModule::background_tasks` returns empty, and HTTP
  responses to `/databases` mutations return `503 mysql_unavailable`
  with a clear error message.

### Requirement: Per-Database RBAC

The databases context SHALL enforce:

- **Owner** (role) — sees and manages all databases.
- **Admin** (role) — sees all databases; can manage databases owned
  by themselves or by users with `User` role.
- **User** (role) — sees and manages only databases they own.

`list_databases(caller)` MUST scope the result set to caller-accessible
records. `delete_database(caller, id)`, `change_password(caller, id)`,
`reveal_password(caller, id)` MUST reject with `Forbidden` when the
caller lacks permission.

#### Scenario: User sees only own databases

- **WHEN** a User calls `list_databases`
- **THEN** the response contains only records where `owner_id == caller.id`.

#### Scenario: Admin cannot reveal Owner-owned DB password

- **WHEN** an Admin calls `reveal_password(caller, owner_owned_db_id)`
- **THEN** the service returns `DatabaseError::Forbidden` and an audit
  event `permission_denied` is recorded.

### Requirement: Audit Trail for Database Mutations

Every database lifecycle event (`database_created`, `database_deleted`,
`database_password_changed`) SHALL be recorded in `audit_log` via the
architecture-owned `AuditService`. Password changes MUST NOT log the
plaintext password — only a boolean metadata flag
(`{"rotated": true}`).

#### Scenario: Create records audit

- **WHEN** a database is created
- **THEN** an audit row is appended with `action='database_created'`,
  `actor=<caller username>`, `target=<database id>`,
  metadata `{"name": "...", "engine": "mysql"}`.

#### Scenario: Password change does NOT leak plaintext

- **WHEN** `change_password` runs
- **THEN** the audit metadata is `{"rotated": true}` and contains NO
  plaintext password.

### Requirement: Database HTTP Routes

The HTTP API SHALL expose:

- `POST /api/v1/databases` — create (admin/owner role required)
- `GET /api/v1/databases` — list (scoped to caller)
- `GET /api/v1/databases/{id}` — fetch metadata only (no password)
- `DELETE /api/v1/databases/{id}` — drop database + user
- `POST /api/v1/databases/{id}/password` — rotate password (returns
  the new plaintext password in the response; only the caller can do this)

#### Scenario: Fetch returns no password

- **WHEN** any caller calls `GET /api/v1/databases/{id}`
- **THEN** the response contains `id`, `name`, `db_user`, `db_host`,
  `engine`, `charset`, `status`, `created_at`, `created_by` — and
  explicitly NO `password` field.

#### Scenario: Rotate returns the new password

- **WHEN** the owner calls `POST /api/v1/databases/{id}/password`
- **THEN** the response includes the newly generated plaintext password
  (a one-time view), and the database row is updated with the new
  ciphertext.

### Requirement: Database CLI

The CLI SHALL expose `openpanel database {create,list,delete,change-password}`.
`change-password` prints the new plaintext password to stdout for the
operator to copy into the site config.

#### Scenario: CLI creates database

- **WHEN** the operator runs `openpanel database create --owner admin
  --name wp --charset utf8mb4`
- **THEN** the database is created, the password is printed to stdout,
  and the operator is reminded to copy it into the site config.

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

