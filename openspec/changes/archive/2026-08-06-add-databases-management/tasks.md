# Tasks: Add Databases Management

## 1. Domain Layer

- [x] 1.1 Create `crates/openpanel-domain/src/databases/mod.rs`
- [x] 1.2 Create `crates/openpanel-domain/src/databases/database.rs`
      with `Database` aggregate
- [x] 1.3 Create `crates/openpanel-domain/src/databases/engine.rs`
      with `DatabaseEngine` (`Mysql`)
- [x] 1.4 Create `crates/openpanel-domain/src/databases/status.rs`
      with `DatabaseStatus` (`Active` / `Suspended`)
- [x] 1.5 Create `crates/openpanel-domain/src/databases/error.rs`
      with `DatabaseError` variants
- [x] 1.6 Create `crates/openpanel-domain/src/databases/repository.rs`
      with `DatabaseRepository` trait
- [x] 1.7 Re-export `databases::*` from `crates/openpanel-domain/src/lib.rs`

## 2. Application — Crypto + MySQL Client

- [x] 2.1 Create `crates/openpanel-app/src/databases/crypto.rs` with
      AES-256-GCM encrypt/decrypt + base64 master key decoding
- [x] 2.2 Unit tests for crypto (roundtrip, tampered ciphertext,
      missing key, wrong length)
- [x] 2.3 Create `crates/openpanel-app/src/databases/mysql.rs` with
      async `MySqlClient` shell-out wrapper
- [x] 2.4 MySQL stderr → `DatabaseError::MysqlError`

## 3. Application — Repository + Service

- [x] 3.1 Create `crates/openpanel-app/src/databases/repo.rs` with
      `SqliteDatabaseRepository` (hex-encoded nonce + ciphertext)
- [x] 3.2 Storage helpers: `encrypt_to_storage` /
      `decrypt_from_storage`
- [x] 3.3 Create `crates/openpanel-app/src/databases/service.rs` with
      `DatabasesService::new(...)`
- [x] 3.4 `create_database(caller, owner_id, owner_username, suffix,
      charset)` — calls MySQL, encrypts, persists, audits
- [x] 3.5 `list_databases`, `get_database`, `delete_database`,
      `change_password`, `reveal_password` with RBAC + audit

## 4. Migrations & Module Wiring

- [x] 4.1 Create
      `crates/openpanel-app/src/migrations/databases/V001__init.sql`
      with `databases` table + indexes
- [x] 4.2 Add `pub const DATABASES_V001` to migrations/mod.rs
- [x] 4.3 Added `DatabaseDeleted` / `DatabasePasswordChanged` audit
      action variants
- [x] 4.4 Create `crates/openpanel-app/src/databases/module.rs` with
      `DatabasesModule::new(ctx, master_key)`
- [x] 4.5 Wire migrations + audit
- [x] 4.6 Re-export `DatabasesModule` from `crates/openpanel-app/src/lib.rs`

## 5. HTTP Routes

- [x] 5.1 Create `crates/openpanel-api/src/dto/database.rs`
- [x] 5.2 Create `crates/openpanel-api/src/routes/databases.rs`
- [x] 5.3 Implement handlers: create, list, get, delete,
      change_password
- [x] 5.4 `DatabaseError -> ApiError` mapping (400/403/404/409/500)
- [x] 5.5 `build_router` accepts `databases: Arc<DatabasesService>`,
      nests under `/api/v1/databases`

## 6. CLI

- [x] 6.1 Add `DatabaseCommand` enum and `database` subcommand
- [x] 6.2 Implement `create_database`, `list_databases`,
      `delete_database`, `change_database_password`
- [x] 6.3 Wire in main.rs

## 7. Composition Root

- [x] 7.1 `serve()` builds `DatabasesModule`, applies migrations
- [x] 7.2 Reads master key from `OPENPANEL__DATABASE__MASTER_KEY`
- [x] 7.3 Passes `databases` to `build_router(...)`
- [x] 7.4 `cargo build --workspace` succeeds
- [x] 7.5 `cargo test --workspace` passes — 30 tests

## 8. Validation

- [x] 8.1 `openspec validate add-databases-management` returns valid
- [x] 8.2 Smoke test: identity + sites + databases migrations
      applied; `GET /api/v1/databases` returns `[]` (200);
      `GET /databases/<bad-uuid>` returns 404; missing mysql → 500;
      bad suffix → 400
- [x] 8.3 Commit + archive via OpenSpec

## Notes

- **`libc` only** (no `users` crate) — /etc/passwd lookup not needed
  here.
- **MySqlClient uses `tokio::process::Command`** — async so it
  integrates with the tokio runtime without blocking.
- **Master key handling** — `load_master_key` reads
  `OPENPANEL__DATABASE__MASTER_KEY` first, falls back to
  `database.master_key` in config. Must be base64 of 32 bytes.
- **SqliteDriver parent-dir fix** — plain paths (no `sqlite://` prefix)
  now also have their parent dir auto-created.