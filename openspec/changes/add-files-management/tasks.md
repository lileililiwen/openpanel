# Tasks: Add Files Management

## 1. Domain Layer

- [ ] 1.1 Create `crates/openpanel-domain/src/files/mod.rs`
- [ ] 1.2 Create `crates/openpanel-domain/src/files/path.rs` with
      `Path` value object: rejects absolute, rejects `..`, rejects
      null byte; normalizes `.` and `/` separators
- [ ] 1.3 Create `crates/openpanel-domain/src/files/file_info.rs`
      with `FileInfo` read model
- [ ] 1.4 Create `crates/openpanel-domain/src/files/error.rs` with
      `FileError` variants: `PathOutsideChroot`, `NotFound`,
      `AlreadyExists`, `IsADirectory`, `IsNotADirectory`,
      `DirectoryNotEmpty`, `FileTooLarge(u64)`, `Forbidden`,
      `Io(String)`, `InvalidPath(String)`, `SiteNotFound(String)`
- [ ] 1.5 Create `crates/openpanel-domain/src/files/repository.rs`
      with `FileRepository` trait (list_dir, read_file, write_file,
      mkdir, remove, rename, chmod)
- [ ] 1.6 Re-export `files::*` from `crates/openpanel-domain/src/lib.rs`

## 2. Application — Filesystem Repository + Service

- [ ] 2.1 Create `crates/openpanel-app/src/files/repo.rs` with
      `FilesystemRepository::new(site: Arc<SitesService>)` and async
      methods using `tokio::fs`
- [ ] 2.2 Implement canonicalize-once-per-call chroot check
- [ ] 2.3 Implement atomic write (write to `.new` + rename) with
      rollback on failure
- [ ] 2.4 Implement `mkdir` (create_dir_all, mode 0755)
- [ ] 2.5 Implement `remove` (file or empty dir; recursive opt-in
      for non-empty dirs)
- [ ] 2.6 Implement `rename` (must be in same chroot)
- [ ] 2.7 Implement `chmod`
- [ ] 2.8 Create `crates/openpanel-app/src/files/service.rs` with
      `FilesService::new(repo, sites_svc, audit)` and methods
      `list_dir`, `read_file`, `write_file`, `mkdir`, `remove`,
      `rename`, `chmod` — all take `caller: &User` + `site_id: Uuid`
      for RBAC + audit
- [ ] 2.9 RBAC check helper reuses sites module's per-site rules

## 3. Audit + Module Wiring

- [ ] 3.1 Add audit action variants: `FileUpdated`, `FileRenamed`,
      `FileModeChanged` (the others already exist)
- [ ] 3.2 Add workspace dep `mime_guess = "2"`
- [ ] 3.3 Create `crates/openpanel-app/src/files/module.rs` with
      `FilesModule::new(ctx, sites_svc)` implementing `Module`
- [ ] 3.4 Re-export `FilesModule` from `crates/openpanel-app/src/lib.rs`

## 4. HTTP Routes

- [ ] 4.1 Create `crates/openpanel-api/src/dto/file.rs` with
      `FileInfoDto`, `ListDirResponse`, `MkdirRequest`,
      `RenameRequest`, `ChmodRequest`, `RemoveRequest` (recursive flag)
- [ ] 4.2 Create `crates/openpanel-api/src/routes/files.rs` with
      `pub fn router(svc: Arc<FilesService>) -> Router`
- [ ] 4.3 Implement `list_dir_or_read_file` (GET on path or root)
- [ ] 4.4 Implement `mkdir_or_upload` (POST: JSON mkdir or multipart
      upload)
- [ ] 4.5 Implement `write_file` (PUT, raw bytes)
- [ ] 4.6 Implement `remove_file_or_dir` (DELETE, optional recursive)
- [ ] 4.7 Implement `rename_file` (PATCH)
- [ ] 4.8 Implement `chmod_file` (POST /chmod subroute)
- [ ] 4.9 Add `FileError -> ApiError` mapping (incl. 413 Payload
      Too Large for `FileTooLarge`)
- [ ] 4.10 Update `crates/openpanel-api/src/router.rs::build_router`
      to nest `/api/v1/sites/{site_id}/files` under the sites router

## 5. CLI

- [ ] 5.1 Add `FileCommand` enum and `file` subcommand in
      `crates/openpanel-cli/src/commands.rs`
- [ ] 5.2 Implement handlers::file_list, file_read, file_write,
      file_mkdir, file_rm, file_rename, file_chmod that resolve
      the site by id-or-domain then call `FilesService` directly
- [ ] 5.3 Wire in main.rs

## 6. Composition Root

- [ ] 6.1 Update `openpanel-cli/src/handlers.rs::serve` to also
      build `FilesModule` (no migrations needed for this module)
- [ ] 6.2 Pass `FilesService` to `build_router(...)`
- [ ] 6.3 `cargo build --workspace` succeeds
- [ ] 6.4 `cargo test --workspace` passes

## 7. Validation

- [ ] 7.1 `openspec validate add-files-management` returns valid
- [ ] 7.2 Smoke: create a site via sites module, then
      `GET /api/v1/sites/{id}/files` returns entries
- [ ] 7.3 Smoke: `PUT /api/v1/sites/{id}/files/test.txt` with body
      `hello` creates the file in the document root
- [ ] 7.4 Smoke: `GET /api/v1/sites/{id}/files/test.txt` returns
      `hello` with `Content-Type: text/plain`
- [ ] 7.5 Smoke: `GET /api/v1/sites/{id}/files/../etc/passwd` returns
      403 `forbidden` (chroot check)
- [ ] 7.6 Commit + archive via OpenSpec