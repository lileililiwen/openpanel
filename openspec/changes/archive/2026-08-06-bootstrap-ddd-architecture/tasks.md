# Tasks: Bootstrap DDD Architecture

## 1. Workspace & Crate Scaffolding

- [x] 1.1 Add `openpanel-core` to `[workspace].members` in root `Cargo.toml`
- [x] 1.2 Create `crates/openpanel-core/Cargo.toml` with deps: `tokio`,
      `axum`, `clap`, `figment`, `jsonschema`, `thiserror`, `async-trait`
- [x] 1.3 Create `crates/openpanel-core/src/lib.rs` re-exporting the
      `Module`, `Config`, `Context`, `DatabaseDriver`, `AuditService`,
      `Job` types
- [x] 1.4 Add `figment`, `jsonschema`, `async-trait`, `walkdir`, `notify`
      to `[workspace.dependencies]`
- [x] 1.5 Confirm `cargo build -p openpanel-domain` succeeds without
      any web/DB/async deps in its tree (proves layering)

## 2. openpanel-core Implementation

- [x] 2.1 Implement `Module` trait in `openpanel-core/src/module.rs`
      with `name`, `config_schema`, `migrations`, `routes`,
      `cli_commands`, `background_tasks`
- [x] 2.2 Implement `ModuleRegistry` holding `Vec<Box<dyn Module>>` and
      `register()` / `iter()` / `route_prefix()`
- [x] 2.3 Implement `Config` in `openpanel-core/src/config.rs` using
      `figment`: defaults → `/etc/openpanel/openpanel.toml` →
      `$OPENPANEL_CONFIG` → `OPENPANEL__*` env vars → CLI overrides
- [x] 2.4 Implement JSON Schema validation: validate final config,
      return descriptive error on failure (exit code 78)
- [x] 2.5 Implement `DatabaseDriver` trait (async `pool`, `begin`) plus
      `SqliteDriver` using `sqlx` with WAL + busy_timeout
- [x] 2.6 Implement migration runner that records applied versions in
      `_migrations` and applies missing ones in lexical order
- [x] 2.7 Implement `tracing_subscriber` init honoring `RUST_LOG` /
      `OPENPANEL__LOG__LEVEL`
- [x] 2.8 Implement `Job` trait + `JobSupervisor` (spawns each
      module's background tasks)
- [x] 2.9 Implement `AuditService` trait + `SqliteAuditService` in
      `openpanel-core/src/audit.rs` (audit_log table owned by
      architecture, not identity)

## 3. openpanel-domain — Common

- [x] 3.1 Create `crates/openpanel-domain/src/common/mod.rs` exporting
      `Email`, `Username`, `Password` value objects with `TryFrom<String>`
      validation (email RFC 5321, username 3-32 alphanum+`-_.`, password
      ≥ 12 chars)
- [x] 3.2 Implement `Password::hash()` using `argon2id` with OWASP 2024
      parameters (m=19456, t=2, p=1) and `verify()` against a stored PHC
- [x] 3.3 Define `DomainError` enum in
      `crates/openpanel-domain/src/common/error.rs` with `thiserror`
      derivations; map to sentinel variants for `Identity`,
      `Validation`, `Persistence`

## 4. openpanel-domain — Identity

- [x] 4.1 Create `crates/openpanel-domain/src/identity/mod.rs` exporting
      `User`, `Session`, `Role`
- [x] 4.2 Implement `Role` enum (`Owner`, `Admin`, `User`) with
      `can_manage_users()`, `can_manage_sites()` helpers
- [x] 4.3 Implement `User` aggregate (`new`, `disable`, `record_login`,
      `change_role`) returning `Result` on invalid input
- [x] 4.4 Implement `Session` aggregate (`new`, `is_expired`,
      `touch`) and opaque `SessionToken` newtype (256-bit, base64url)
- [x] 4.5 Define `UserRepository` and `SessionRepository` **traits** in
      `crates/openpanel-domain/src/identity/repository.rs` — async fns
      only, no `sqlx`
- [x] 4.6 Define `IdentityError` in
      `crates/openpanel-domain/src/identity/error.rs` with variants
      `InvalidCredentials`, `AccountDisabled`, `PasswordTooShort`,
      `UserNotFound`, `UsernameTaken`, `EmailTaken`, `SessionExpired`

## 5. openpanel-app — Migrations & Identity Service

- [x] 5.1 Create `crates/openpanel-app/src/migrations/000_audit.sql`
      defining `audit_log`
- [x] 5.2 Create `crates/openpanel-app/src/migrations/identity/V001__init.sql`
      defining `users` and `sessions` tables per the architecture spec
- [x] 5.3 Implement `SqliteUserRepository` in
      `crates/openpanel-app/src/identity/repo.rs` implementing
      `UserRepository`
- [x] 5.4 Implement `SqliteSessionRepository` in the same file
- [x] 5.5 Implement `IdentityService` in
      `crates/openpanel-app/src/identity/service.rs` with `create_user`,
      `login`, `logout`, `list_users`, `change_role`, `disable_user`,
      `delete_user` — wraps repositories, calls `audit.record(...)`
- [x] 5.6 Implement `IdentityModule` (impls `Module` trait) wiring the
      service, migrations

## 6. openpanel-api — HTTP Surface

- [x] 6.1 Create `crates/openpanel-api/src/dto/auth.rs` with
      `LoginRequest`, `LoginResponse`, `UserDto`, `CreateUserRequest`,
      `ErrorBody` (using `serde`)
- [x] 6.2 Create `crates/openpanel-api/src/middleware/session.rs`
      resolving bearer token or `openpanel_session` cookie, loading the
      user, injecting `AuthSession` into request extensions
- [x] 6.3 Create `crates/openpanel-api/src/extract.rs` with `AuthUser`
      extractor and `RequireOwner` extractor returning 403 if mismatched
- [x] 6.4 Create `crates/openpanel-api/src/routes/identity.rs` with
      handlers for `/login`, `/logout`, `/me`, `/users`,
      `/users/{id}`, `/users/{id}/disable`, `/users/{id}/password`
- [x] 6.5 Implement `IntoResponse for ApiError` (wraps `IdentityError`)
      mapping to 400/401/403/404/409 with `ErrorBody` JSON
- [x] 6.6 Implement `build_router(identity)` returning an `axum::Router` with
      `/api/v1/identity/*` mount points
- [x] 6.7 Mount `session_middleware` on the `/api/v1` subtree

## 7. openpanel-cli

- [x] 7.1 Define root `clap` command with subcommands `serve`, `migrate`,
      `user`
- [x] 7.2 Implement `openpanel serve` — loads config, opens DB, runs
      migrations, starts API + audit
- [x] 7.3 Implement `openpanel migrate` — runs migrations and exits
- [x] 7.4 Implement `openpanel user create|list|disable|delete` calling
      `IdentityService` directly
- [x] 7.5 Embedded `audit.sql` for the architecture-owned audit table

## 8. openpanel-agent

- [x] 8.1 Implement `openpanel-agent` binary that calls into the same
      `serve` handler (single-host bundled mode by default)
- [x] 8.2 Stub `BackgroundTask` trait + `JobSupervisor` in
      `openpanel-core` for future cron/monitoring

## 9. Composition Root

- [x] 9.1 `crates/openpanel-cli/src/main.rs` builds the registry, loads
      config, runs migrations, starts the server
- [x] 9.2 Confirmed the binary starts and prints
      `openpanel listening on http://0.0.0.0:8080` (see validation log)

## 10. Validation & Documentation

- [x] 10.1 `cargo build --workspace --release` succeeds
- [x] 10.2 `cargo test --workspace` passes (9 unit tests: Email, Username,
      Password (argon2id), Role permissions, Session expiry math,
      migration parsing)
- [x] 10.3 Manual smoke: `openpanel user create` works against a fresh DB
- [x] 10.4 `POST /api/v1/identity/login` returns a session token
- [x] 10.5 `GET /api/v1/identity/me` returns the admin user; bad password
      returns 401 `invalid_credentials`
- [x] 10.5b Admin attempting user creation returns 403 `forbidden`
- [x] 10.6 `openspec validate bootstrap-ddd-architecture` passes
- [x] 10.7 Root `README.md` written