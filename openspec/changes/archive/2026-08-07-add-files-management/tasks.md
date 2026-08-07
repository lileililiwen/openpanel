# Tasks: Add Files Management

## 1. Domain Layer

- [x] 1.1 Create `crates/openpanel-domain/src/files/mod.rs`
- [x] 1.2 Create `crates/openpanel-domain/src/files/path.rs` with
      `Path` value object
- [x] 1.3 Create `crates/openpanel-domain/src/files/file_info.rs`
- [x] 1.4 Create `crates/openpanel-domain/src/files/error.rs`
- [x] 1.5 Create `crates/openpanel-domain/src/files/repository.rs`
- [x] 1.6 Re-export `files::*` from `crates/openpanel-domain/src/lib.rs`

## 2. Application — Filesystem Repository + Service

- [x] 2.1 Create `crates/openpanel-app/src/files/repo.rs` with
      `FilesystemRepository` + free `canonicalize_chroot` and
      `resolve_path` helpers
- [x] 2.2 Canonicalize-once-per-call chroot check
- [x] 2.3 Atomic write (write to `.new` + rename)
- [x] 2.4 `mkdir` (create_dir_all, mode 0755)
- [x] 2.5 `remove` (file or empty dir; recursive opt-in)
- [x] 2.6 `rename` (must be in same chroot)
- [x] 2.7 `chmod`
- [x] 2.8 `FilesService` with `list_dir`, `read_file`, `write_file`,
      `mkdir`, `remove`, `rename`, `chmod` (caller + site_id for RBAC)
- [x] 2.9 RBAC reuses sites module's per-site rules

## 3. Audit + Module Wiring

- [x] 3.1 Added audit action variants: `FileUpdated`,
      `FileRenamed`, `FileModeChanged`
- [x] 3.2 Workspace dep `mime_guess` (for FileInfo mime hints)
- [x] 3.3 `FilesModule::new(ctx)` implementing `Module`
- [x] 3.4 Re-exported `FilesModule` from openpanel-app

## 4. HTTP Routes

- [x] 4.1 `crates/openpanel-api/src/dto/file.rs` (FileInfoDto,
      ListDirResponse, RemoveRequest)
- [x] 4.2 `crates/openpanel-api/src/routes/files.rs` with
      `pub fn router(svc: Arc<FilesService>) -> Router`
- [x] 4.3 `GET /{site_id}` lists root; `GET /{site_id}/{*path}` lists dir or reads file
- [x] 4.4 `POST /{site_id}/{*path}` multipart upload
- [x] 4.5 `PUT /{site_id}/{*path}` raw write
- [x] 4.6 `DELETE /{site_id}/{*path}?recursive=bool` remove
- [x] 4.7 `PATCH /{site_id}/{*path}` rename + chmod (folded into PATCH body)
- [x] 4.8 Chmod via PATCH (folded; axum 0.8 disallows wildcard + sub-path)
- [x] 4.9 `FileError -> ApiError` mapping incl. 413
- [x] 4.10 `build_router` accepts `files: Arc<FilesService>`, nests
      under `/api/v1/files`

## 5. CLI

- [x] 5.1 `FileCommand` enum and `file` subcommand
- [x] 5.2 handlers::file_list, file_read, file_write, file_mkdir,
      file_rm, file_rename, file_chmod
- [x] 5.3 Wired in main.rs

## 6. Composition Root

- [x] 6.1 serve() builds FilesModule
- [x] 6.2 build_router takes (identity, sites, databases, files)
- [x] 6.3 cargo build succeeds
- [x] 6.4 cargo test --workspace passes (36 tests)

## 7. Validation

- [x] 7.1 openspec validate returns valid
- [x] 7.2 Smoke: list root 200, write 200, read 200 with body,
      list-after-write 200, rename 200, delete 200,
      list-after-delete 200, path-traversal 400
- [x] 7.3 Commit + archive via OpenSpec

## Design notes worth flagging

- **axum 0.8 wildcard**: `/{*path}/chmod` is rejected (wildcard +
  literal sub-path). Folded chmod into PATCH body — `{"mode": "0755"}`
  or `{"to": "new"}`.
- **Path validation**: `Path::new` rejects absolute, `..`, null bytes.
  Resolution uses `std::fs::canonicalize` on the chroot + each path
  candidate; escapes are caught by `starts_with(chroot_canonical)` check.
- **MIME guessing**: instead of pulling the `mime_guess` crate into
  the API crate, the API layer uses a small in-tree extension map
  (html, css, js, json, txt, png, jpg, gif, svg, pdf). The repo still
  uses mime_guess for FileInfo listings.
- **Resolved RFC 3339 bug**: The smoke test inserted sites via SQLite
  with `datetime('now')` (legacy format) — the Rust repo expects
  RFC 3339. Updated the smoke script to use `date -u +%Y-%m-%dT%H:%M:%SZ`.
  In production, all inserts go through the repo which uses
  `to_rfc3339()` consistently.