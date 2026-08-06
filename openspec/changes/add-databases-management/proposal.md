# Add Databases Management

## Why

Sites management ships in v0.1-alpha +sites but operators still can't
provision a MySQL database for a site. To run a WordPress / Ghost /
custom-PHP site, the operator needs a database, a database user, and
the ability to set/rotate that user's password. Without this, sites are
purely static.

This change adds the `databases` bounded context: provision a MySQL
database + DB user tied to a panel user, encrypted credential storage,
and the same per-user RBAC as sites.

## What Changes

- New `Database` aggregate in `openpanel-domain/databases/` (domain):
  id, owner_id, name (DB name), db_user, db_host (`localhost` /
  socket), engine (`mysql` for v0.1), charset (`utf8mb4` default),
  status, audit fields. The DB user's password is NOT in the aggregate
  — it lives in an encrypted `password_ciphertext` column on the
  repository side, decrypted only on demand.
- New `DatabaseRepository` trait in domain; SQLite adapter in
  `openpanel-app/databases/repo.rs`.
- New `DatabasesService` in `openpanel-app/databases/service.rs`:
  `create_database`, `list_databases`, `get_database`, `delete_database`,
  `change_password`, `reveal_password`. Mutations shell out to the
  `mysql` CLI (`CREATE DATABASE`, `CREATE USER`, `GRANT`,
  `DROP DATABASE`, `DROP USER`, `SET PASSWORD`).
- New `MySqlClient` in `openpanel-app/databases/mysql.rs` wrapping the
  shell. Returns structured errors (`MysqlMissing`, `MysqlError`,
  `PermissionDenied`). Detect presence via `which mysql`.
- New password encryption in `openpanel-app/databases/crypto.rs`
  using AES-256-GCM with a master key from config. Each record has its
  own random nonce.
- New `DatabasesModule` (impls `Module`) wiring service, migrations.
- New HTTP routes under `/api/v1/databases/*`:
  - `POST /databases` create
  - `GET /databases` list (RBAC-scoped)
  - `GET /databases/{id}` fetch
  - `DELETE /databases/{id}` drop
  - `POST /databases/{id}/password` rotate password (returns new password)
- New CLI subcommand `openpanel database {create,list,delete,change-password}`.
- New migration `databases/V001__init.sql`.
- New config section `database.master_key` (env: `OPENPANEL__DATABASE__MASTER_KEY`)
  — base64-encoded 32 bytes. Required for the databases module to start.

## Capabilities

### New Capabilities

- `databases` — MySQL database + DB user provisioning, encrypted
  password storage at rest, per-user RBAC.

### Modified Capabilities

- `architecture` — no behavioral change; just confirms that the new
  module follows the existing `Module` trait contract.

## Impact

- `crates/openpanel-domain/src/databases/` — Database aggregate,
  DatabaseEngine, DatabaseStatus, DatabaseError, DatabaseRepository trait
- `crates/openpanel-app/src/databases/` — DatabasesService, repo,
  MySqlClient, crypto, DatabasesModule
- `crates/openpanel-app/src/migrations/databases/V001__init.sql`
- `crates/openpanel-api/src/routes/databases.rs` + `dto/database.rs`
- `crates/openpanel-api/src/router.rs` — nest `/api/v1/databases`
- `crates/openpanel-cli/src/commands.rs` + `handlers.rs` — `database`
  subcommand
- `crates/openpanel-core/src/audit.rs` — already has `DatabaseCreated`
  variant; add `DatabaseDeleted`, `DatabasePasswordChanged`
- New workspace dep: `aes-gcm = "0.10"`
- New domain error mapping in `crates/openpanel-api/src/error.rs`