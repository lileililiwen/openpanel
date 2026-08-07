# Design: Add Files Management

## Context

Sites and databases ship in v0.1-alpha. To deploy real content into a
site's document root (`/var/www/<domain>/public_html`), the operator
needs a chrooted file manager — list, read, write, upload, delete,
rename, chmod, mkdir. This change adds the `files` bounded context.

The `files` module depends on `sites` because every file operation
is scoped to a site's document root. The RBAC rules are the same as
the sites module (Owner / Admin / User).

## Goals / Non-Goals

**Goals:**

- Chroot every filesystem operation to the site's document root —
  absolute path-traversal resistance.
- Pure-Rust filesystem ops (`std::fs`) — no shell-out needed.
- RBAC mirroring the sites module.
- Audit every mutation with the relative path; never log file
  contents.
- Single HTTP route family (`/api/v1/sites/{id}/files/...`) that
  covers all operations.

**Non-Goals:**

- Symlink following across chroots — symlinks pointing outside the
  chroot are rejected outright.
- In-browser editor, image preview, zip/unzip, search — all deferred
  to v0.2+.
- Inotify / file-watch for live reload — deferred.
- SFTP / FTP — out of scope (use SSH for terminal access).

## Decisions

### 1. Pure-Rust filesystem ops, no shell-out

**Decision**: Use `std::fs`, `tokio::fs` (for async variants),
`std::os::unix::fs::PermissionsExt`. No shell-out to `ls`, `cat`, etc.

**Rationale**: Filesystem syscalls are universally available on the
target platform; no dependency on shell tools. Atomic rename via
`std::fs::rename` works on the same filesystem. Async variants are
available via `tokio::fs` for the upload/write paths.

### 2. Canonicalize once per request

**Decision**: Canonicalize the chroot at the start of each request via
`std::fs::canonicalize`. Then for every `Path` candidate, join with
the chroot, canonicalize that, and verify the result starts with the
chroot prefix byte-for-byte. Symlinks pointing outside the chroot
fail this check.

**Rationale**: One canonicalization per request is cheap and
eliminates the entire class of path-traversal attacks
(`../`, `..%2F`, symlinks, double-slashes, null bytes).

### 3. Atomic write via temp + rename

**Decision**: `write_file` writes to `<path>.new` then renames to
the final path. If the target already exists, snapshot the previous
content into memory; on failure, restore.

**Rationale**: Mirrors the nginx config generator pattern (atomic
write). Prevents partial files if the write fails mid-stream.

### 4. Read limit at 50 MB

**Decision**: `read_file` enforces a 50 MB cap. The HTTP layer maps
`FileError::FileTooLarge` to `413 Payload Too Large` with the actual
size in the response body.

**Rationale**: Operators should use `scp` or `rsync` for bulk
transfers. The in-panel file manager is for editing a few files at a
time. 50 MB is generous for that use case.

### 5. Multipart upload via `axum::extract::Multipart`

**Decision**: POST to `/sites/{id}/files/{*path}` with
`Content-Type: multipart/form-data` and a single `file` field is
treated as an upload. Same path with `Content-Type:
application/json` is treated as a mkdir.

**Rationale**: REST convention. axum ships multipart support out of
the box; no extra dep.

### 6. No new database table

**Decision**: The files module does NOT persist any state. All state
lives on the filesystem. The only database interaction is the audit
log.

**Rationale**: Files are the filesystem; persisting a parallel index
in SQLite would drift. The audit log is the only durable record of
mutations.

### 7. CLI uses site domain or id

**Decision**: `openpanel file <op> --site <id-or-domain>` resolves
the site via the sites service, then operates on its document root.

**Rationale**: Operators usually know the domain, not the UUID. The
CLI accepts both and looks up by domain first, then by id.

## Risks / Trade-offs

- **Risk**: Canonicalize fails on non-existent paths (during
  write/mkdir) → *Mitigation*: For write/mkdir, canonicalize the
  parent directory and check it starts with the chroot; the new
  basename is then appended unchecked (no `..` allowed in basename).
- **Risk**: Concurrent writes to the same file → *Mitigation*:
  Last-writer-wins. Documented in the spec. v0.2 can add a per-file
  mutex.
- **Risk**: File manager leaks secrets via audit metadata → *Mitigation*:
  Audit metadata only contains `{path, bytes}` — never file contents.
  Tested in unit tests.
- **Risk**: Path contains a null byte → *Mitigation*: `Path`
  constructor rejects `\0` explicitly.

## Migration Plan

- No data migration needed (no new tables).
- Operator must ensure `openpanel` process has read/write access to
  each site's document root. v0.1 assumes the panel runs as root.
- The sites module's `document_root` is the chroot — no extra config.

## Open Questions

- Should `read_file` return JSON (with base64 contents) or raw bytes?
  → *Default: raw bytes*; clients that want JSON can wrap. JSON
  would be wasteful for images.
- Should we support streaming uploads for >50 MB? → *Default: no*
  for v0.1; recommend `scp` for bulk. v0.2 can add chunked uploads.