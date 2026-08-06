# Tasks: Add Databases Management

## 1. Domain Layer

- [ ] 1.1 Create `crates/openpanel-domain/src/databases/mod.rs`
      re-exporting `Database`, `DatabaseEngine`, `DatabaseStatus`,
      `DatabaseError`, `DatabaseRepository`
- [ ] 1.2 Create `crates/openpanel-domain/src/databases/database.rs`
      with `Database` aggregate (`new`, `restore`, validation, `suspend`,
      `resume`)
- [ ] 1.3 Create `crates/openpanel-domain/src/databases/engine.rs` with
      `DatabaseEngine` enum (`Mysql`) + `FromStr` + `as_str`
- [ ] 1.4 Create `crates/openpanel-domain/src/databases/status.rs`
      with `DatabaseStatus` enum (`Active`, `Suspended`)
- [ ] 1.5 Create `crates/openpanel-domain/src/databases/error.rs` with
      `DatabaseError` variants: `InvalidName`, `InvalidCharset`,
      `DuplicateDatabase`, `NotFound`, `Forbidden`, `MysqlMissing`,
      `MysqlError(String)`, `MasterKeyMissing`, `Encryption(String)`,
      `Decryption(String)`, `Persistence(String)`, `Io(String)`
- [ ] 1.6 Create `crates/openpanel-domain/src/databases/repository.rs`
      with `DatabaseRepository` trait (insert, find_by_id,
      find_by_name, list_all, list_by_owner, update_password_ciphertext,
      update_status, delete, count)
- [ ] 1.7 Re-export `databases::*` from `crates/openpanel-domain/src/lib.rs`

## 2. Application — Crypto + MySQL Client

- [ ] 2.1 Create `crates/openpanel-app/src/databases/crypto.rs` with
      `encrypt(key, plaintext) -> (nonce, ciphertext)` and
      `decrypt(key, nonce, ciphertext) -> plaintext` using `aes-gcm`
      AES-256-GCM
- [ ] 2.2 Add unit tests for crypto roundtrip + tampered ciphertext
- [ ] 2.3 Create `crates/openpanel-app/src/databases/mysql.rs` with
      `MySqlClient::new(mysql_binary, admin_user)` and methods:
      `create_database`, `create_user`, `grant_all`, `change_password`,
      `drop_database`, `drop_user`, `available` (detects `mysql` via
      `which mysql`)
- [ ] 2.4 Map MySQL stderr to `DatabaseError::MysqlError`

## 3. Application — Repository + Service

- [ ] 3.1 Create `crates/openpanel-app/src/databases/repo.rs` with
      `SqliteDatabaseRepository` implementing `DatabaseRepository`,
      storing `nonce` + `ciphertext` as concatenated hex string in
      `password_ciphertext` column
- [ ] 3.2 Add helper `encrypt_for_storage(key, plaintext) -> String`
      and `decrypt_from_storage(key, stored) -> String` using the
      crypto module
- [ ] 3.3 Create `crates/openpanel-app/src/databases/service.rs` with
      `DatabasesService::new(repo, mysql, audit, master_key)`
- [ ] 3.4 Implement `create_database(caller, owner_id, name, charset)`
      — validates, calls MySQL to create db + user + grant, encrypts
      password, persists, audits, returns plaintext password
- [ ] 3.5 Implement `list_databases(caller)` (RBAC scoped),
      `get_database(caller, id)` (metadata only), `delete_database`
      (RBAC + MySQL drop), `change_password(caller, id)` (RBAC +
      MySQL ALTER USER + re-encrypt), `reveal_password(caller, id)`
      (RBAC + decrypt + audit)

## 4. Migrations & Module Wiring

- [ ] 4.1 Create
      `crates/openpanel-app/src/migrations/databases/V001__init.sql`
      with `databases` table (id, owner_id, name unique, db_user,
      db_host, engine, charset, status, password_ciphertext,
      created_at, created_by)
- [ ] 4.2 Add `pub const DATABASES_V001` to migrations/mod.rs
- [ ] 4.3 Add `DatabaseCreated`/`DatabaseDeleted`/
      `DatabasePasswordChanged` audit action variants (the first
      already exists; add the other two)
- [ ] 4.4 Create `crates/openpanel-app/src/databases/module.rs` with
      `DatabasesModule::new(ctx)` returning a struct that implements
      `openpanel_core::Module`
- [ ] 4.5 Wire migrations + audit
- [ ] 4.6 Re-export `DatabasesModule` from `crates/openpanel-app/src/lib.rs`

## 5. HTTP Routes

- [ ] 5.1 Create `crates/openpanel-api/src/dto/database.rs` with
      `CreateDatabaseRequest`, `DatabaseDto`, `CreatedDatabaseResponse`
      (includes plaintext password once)
- [ ] 5.2 Create `crates/openpanel-api/src/routes/databases.rs` with
      `pub fn router(svc: Arc<DatabasesService>) -> Router`
- [ ] 5.3 Implement handlers: `create_database`, `list_databases`,
      `get_database`, `delete_database`, `change_password`
- [ ] 5.4 Add `DatabaseError -> ApiError` mapping in
      `crates/openpanel-api/src/routes/databases.rs`
- [ ] 5.5 Update `crates/openpanel-api/src/router.rs::build_router` to
      accept `databases: Arc<DatabasesService>` and nest under
      `/api/v1/databases`

## 6. CLI

- [ ] 6.1 Add `DatabaseCommand` enum and `database` subcommand in
      `crates/openpanel-cli/src/commands.rs`
- [ ] 6.2 Implement `handlers::create_database`, `list_databases`,
      `delete_database`, `change_database_password` calling
      `DatabasesService`
- [ ] 6.3 Wire the new subcommand in `crates/openpanel-cli/src/main.rs`

## 7. Composition Root

- [ ] 7.1 Update `openpanel-cli/src/handlers.rs::serve` to also build
      `DatabasesModule` and register its migrations
- [ ] 7.2 Read `database.master_key` from config; abort if missing for
      the databases module
- [ ] 7.3 Pass `databases` service into `build_router(...)`
- [ ] 7.4 `cargo build --workspace` succeeds
- [ ] 7.5 `cargo test --workspace` passes

## 8. Validation

- [ ] 8.1 `openspec validate add-databases-management` returns valid
- [ ] 8.2 Manual smoke: server starts, `GET /api/v1/databases` returns
      empty array, `GET /databases/<bad-id>` returns 404
- [ ] 8.3 Commit + archive via OpenSpec