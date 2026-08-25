## MODIFIED Requirements

### Requirement: Database Aggregate

The databases context SHALL model a `Database` aggregate with fields:

- `id` — UUID v4
- `owner_id` — UUID of the owning user (FK to identity.users)
- `name` — the MySQL database name; panel-enforced as
  `{owner_username}_{requested_name}` to guarantee uniqueness and
  ownership traceability. Lowercase, `[a-z0-9_]{3,32}`.
- `db_user` — the MySQL username; same format as `name`
- `db_host` — `localhost` by default; additional host patterns are
  derived from the database's RemoteAccess ACL and tracked alongside
  it
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

### Requirement: MySQL Provisioning via Shell

The databases context SHALL use the `mysql` CLI to provision, modify,
and drop databases and users. All mutations MUST shell out:

- Create: `mysql -e "CREATE DATABASE ..."` then
  `mysql -e "CREATE USER ... IDENTIFIED BY '...'"`
  then `mysql -e "GRANT ALL ON db.* TO user@'localhost'"`, plus one
  `CREATE USER ... GRANT` per host pattern derived from the
  database's RemoteAccess ACL.
- Change password: `mysql -e "ALTER USER ... IDENTIFIED BY '...'"`.
- Drop: `mysql -e "DROP DATABASE ..."` then
  `DROP USER` for every account of the user (local and remote
  patterns).

The `mysql` binary MUST be detected at startup via `which mysql`;
if absent, the service returns `DatabaseError::MysqlMissing` on every
mutation and refuses to boot.

#### Scenario: MySQL missing fails gracefully

- **WHEN** `mysql` is not in PATH
- **THEN** the service refuses to create / drop databases, the
  `DatabasesModule::background_tasks` returns empty, and HTTP
  responses to `/databases` mutations return `503 mysql_unavailable`
  with a clear error message.

## ADDED Requirements

### Requirement: Remote-Access Grant Enforcement

Applying a database's RemoteAccess ACL SHALL create or remove MySQL
accounts (`user@pattern`) to match the ACL exactly; application SHALL
be all-or-nothing per database and failures SHALL leave stored state
unchanged and return `DatabaseError::GrantFailed`.

#### Scenario: Enable grants accounts

- **WHEN** an Owner enables remote access for CIDR
        `203.0.113.0/24`
- **THEN** account `{db_user}@203.0.113.%` exists with grants on that
        database only, and audit records the applied pattern.

#### Scenario: Disable removes accounts

- **WHEN** remote access is disabled
- **THEN** every non-local account for the user is dropped and only
        `user@localhost` remains.

#### Scenario: Partial failure rolls back

- **WHEN** the second of three grant statements fails
- **THEN** the first is reverted, the ACL keeps its previous value,
        and the error names the failed step.

### Requirement: Boot-Time Grant Reconcile

On startup the service SHALL reconcile stored ACLs against actual
MySQL accounts and heal drift caused by out-of-band deletion or
creation.

#### Scenario: Manually deleted grant healed

- **WHEN** an operator deletes a remote account directly in MySQL and
        the panel restarts
- **THEN** reconcile recreates the account to match the stored ACL
        and audits the correction.

### Requirement: Global Access Gate

A CIDR of `0.0.0.0/0` (or prefix shorter than /16) SHALL be accepted
only when the deployment explicitly opts in via configuration;
otherwise the surface rejects it with `global_access_locked`.

#### Scenario: Global CIDR without opt-in

- **WHEN** a caller submits `0.0.0.0/0` on a default install
- **THEN** the response is 422 `global_access_locked` and nothing is
        applied.

### Requirement: Remote-Access Surfaces

Owners SHALL manage remote access per database via API, CLI, and web;
responses SHALL contain CIDRs and applied patterns but never
passwords.

#### Scenario: Round-trip

- **WHEN** an Owner PUTs an ACL then GETs it
- **THEN** the response lists the same CIDRs with their derived
          patterns and no credential material.
