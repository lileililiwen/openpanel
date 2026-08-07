# Add Files Management

## Why

Sites and databases ship in v0.1-alpha. Operators can provision vhosts
and MySQL databases but cannot upload the actual site contents. To
deploy a WordPress theme, a static site, or any web content, the
operator needs a file manager scoped to a site's document root.

This change adds the `files` bounded context: a chrooted file manager
for sites. Every operation is restricted to the site's document root
(no path traversal), and every mutation is audited.

## What Changes

- New `Path` value object in `openpanel-domain/files/` (domain):
  validates that a relative path stays inside a chroot after
  normalization (no `..`, no absolute paths, no symlink escape).
- New `FileInfo` aggregate read model: name, is_dir, size, mode,
  mtime, mime hint.
- New `FileRepository` trait in domain with methods: list, read,
  write, mkdir, remove, rename, chmod. SQLite is NOT used — the
  repository is a filesystem-backed adapter.
- New `FilesService` in `openpanel-app/files/service.rs`:
  `list_dir`, `read_file`, `write_file`, `mkdir`, `remove`,
  `rename`, `chmod`. Uses `std::fs` directly (no shell-out needed
  for filesystem ops).
- New `FilesModule` (impls `Module`) wiring the service.
- New HTTP routes under `/api/v1/sites/{site_id}/files/...`:
  - `GET /sites/{id}/files/{*path}` — list dir OR read file based on path type
  - `POST /sites/{id}/files/{*path}` — mkdir (dir mode) OR upload (multipart)
  - `PUT /sites/{id}/files/{*path}` — write text content (raw body)
  - `DELETE /sites/{id}/files/{*path}` — remove file or empty dir
  - `PATCH /sites/{id}/files/{*path}` — rename (`{"to": "..."}`)
  - `POST /sites/{id}/files/{*path}/chmod` — change mode (`{"mode": "0755"}`)
- New CLI subcommand: `openpanel file {list,read,write,mkdir,rm,rename,chmod}`
  operating on a given site.
- New audit actions: `FileUploaded`, `FileUpdated`, `FileDeleted`,
  `FileRenamed`, `FileModeChanged`. (`FileUploaded` and `FileDeleted`
  already exist from earlier changes.)

## Capabilities

### New Capabilities

- `files` — chrooted file manager for sites. All operations are
  path-traversal-safe and RBAC-scoped to the site's owner.

### Modified Capabilities

- `sites` — no behavioral change. `Site.document_root` is the chroot
  for the file manager; this change merely consumes it.

## Impact

- `crates/openpanel-domain/src/files/` — `Path`, `FileInfo`,
  `FileRepository`, `FileError`
- `crates/openpanel-app/src/files/` — `FilesService`,
  `FilesRepository` (filesystem-backed), `FilesModule`
- `crates/openpanel-api/src/routes/files.rs` + `dto/file.rs`
- `crates/openpanel-api/src/router.rs` — nest under
  `/api/v1/sites/{id}/files`
- `crates/openpanel-cli/src/commands.rs` + `handlers.rs` — `file`
  subcommand
- `crates/openpanel-core/src/audit.rs` — add `FileUpdated`,
  `FileRenamed`, `FileModeChanged` variants
- Workspace deps: `mime_guess = "2"` (for Content-Type hints), and
  `tokio-util` for `tokio::io::AsyncReadExt` (already transitively
  present via axum)