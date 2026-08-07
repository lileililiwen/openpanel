# files Specification

## Purpose
TBD - created by archiving change add-files-management. Update Purpose after archive.
## Requirements
### Requirement: Path Value Object

The files context SHALL model a `Path` value object representing a
**relative** path inside a site's document root. `Path::new(relative)`
MUST:

- Reject absolute paths (anything starting with `/`).
- Reject parent-directory references (`..`).
- Reject empty paths (the chroot itself is not a valid target — use
  an explicit `""` for root listing via a separate `Path::root()`
  constructor).
- Normalize the path by collapsing `.` and `/` separators.

A `Path` MUST always be paired with its chroot (`Site.document_root`)
when resolving to an absolute filesystem path. The chroot MUST be
canonicalized once via `std::fs::canonicalize` at session start, then
every `Path` is checked by canonicalizing the joined candidate and
verifying it begins with the chroot prefix.

#### Scenario: Reject absolute path

- **WHEN** the API receives a request for `/sites/.../files//etc/passwd`
- **THEN** the service returns `FileError::PathOutsideChroot` and the
  filesystem is NOT touched.

#### Scenario: Reject parent traversal

- **WHEN** the API receives a request for
  `/sites/.../files/../../etc/passwd`
- **THEN** the service returns `FileError::PathOutsideChroot`.

#### Scenario: List root

- **WHEN** the API receives `GET /sites/{id}/files` (no path after
  `/files`)
- **THEN** the service lists the site's document root.

### Requirement: FileInfo Read Model

The files context SHALL expose a `FileInfo` struct describing each
entry in a directory listing:

- `name` — basename, not full path
- `is_dir` — boolean
- `size` — bytes (0 for directories)
- `mode` — octal string (e.g. `"0755"`)
- `mtime` — RFC 3339 timestamp
- `mime` — best-guess MIME type (via the `mime_guess` crate; `None`
  for directories)

#### Scenario: Listing a populated directory

- **WHEN** the service lists `/var/www/example.com/public_html` with
  three entries (`index.html`, `style.css`, `images/`)
- **THEN** the response contains three `FileInfo` records with the
  correct names, sizes, modes, and `mime` set for the two files
  (`text/html`, `text/css`) and `None` for `images/`.

### Requirement: Filesystem Operations

The files context SHALL provide:

- `list_dir(site, relative_path)` — returns `Vec<FileInfo>`
- `read_file(site, relative_path)` — returns bytes (max 50 MB)
- `write_file(site, relative_path, bytes)` — atomic write
  (write-to-temp + rename)
- `mkdir(site, relative_path)` — creates dir, mode 0755 default
- `remove(site, relative_path)` — removes file or empty dir; refuses
  non-empty dirs unless `recursive: true` flag is passed
- `rename(site, from, to)` — atomic rename; both paths must be in the
  same chroot
- `chmod(site, relative_path, mode)` — change file mode

All operations MUST log to `audit_log` with the appropriate action
variant and the relative path.

#### Scenario: Upload a file

- **WHEN** the operator uploads `style.css` to
  `/sites/{id}/files/wp-content/themes/2021/style.css`
- **THEN** the file is created at
  `/var/www/{domain}/public_html/wp-content/themes/2021/style.css`
  with mode 0644, and an audit row with `action='file_uploaded'`,
  `target=<site id>`, `metadata={"path":"wp-content/themes/2021/style.css"}`
  is recorded.

#### Scenario: Read a 100 MB file should fail

- **WHEN** the operator requests to read a file larger than 50 MB
- **THEN** the service returns `FileError::FileTooLarge` with the
  actual size in the message.

#### Scenario: Remove non-empty dir without recursive flag

- **WHEN** the operator requests `DELETE /sites/{id}/files/wp-content`
  and the dir contains files
- **THEN** the service returns `FileError::DirectoryNotEmpty` and
  the directory is NOT removed.

### Requirement: Per-Site RBAC

The files context SHALL reuse the sites RBAC. `FilesService` methods
accept `&User` and `&Site`; the service verifies the caller can
manage the site before performing any filesystem operation. The check
mirrors `SitesService::assert_can_manage`:

- **Owner** (role) — all sites
- **Admin** (role) — any site
- **User** (role) — only sites they own

#### Scenario: User role blocked from another user's site files

- **WHEN** a User role calls
  `GET /sites/{other_user_site_id}/files/index.html`
- **THEN** the service returns `FileError::Forbidden` and no
  filesystem syscall is made.

### Requirement: Audit Trail

Every filesystem mutation SHALL be recorded in `audit_log` via the
architecture-owned `AuditService`. The metadata SHALL include the
relative path; it SHALL NEVER include file contents.

#### Scenario: Write audit metadata

- **WHEN** the operator writes `/sites/{id}/files/index.html`
- **THEN** the audit row contains
  `{"path":"index.html","bytes":1234}` — NOT the file contents.

### Requirement: File HTTP Routes

The HTTP API SHALL expose:

- `GET    /api/v1/sites/{id}/files/{*path}` — list dir or read file
- `POST   /api/v1/sites/{id}/files/{*path}` — mkdir (Content-Type
  `application/json` with `{"recursive": bool}`) or upload
  (multipart/form-data with a single `file` field)
- `PUT    /api/v1/sites/{id}/files/{*path}` — write raw bytes
  (Content-Type chosen by client, default `application/octet-stream`)
- `DELETE /api/v1/sites/{id}/files/{*path}` — remove
- `PATCH  /api/v1/sites/{id}/files/{*path}` — rename (`{"to": "..."}`)
- `POST   /api/v1/sites/{id}/files/{*path}/chmod` — change mode
  (`{"mode": "0755"}`)

Responses for `GET` on a directory SHALL be JSON
`{"entries": [FileInfo, ...]}`. Responses for `GET` on a file SHALL
be the raw bytes with `Content-Type` set to the guessed mime.

#### Scenario: List root returns JSON

- **WHEN** a caller does `GET /api/v1/sites/{id}/files` on a site
  with 3 entries in the document root
- **THEN** the response is
  `{"entries":[{"name":"...","is_dir":false,...}, ...]}` with 200 OK.

#### Scenario: Read a file returns raw bytes

- **WHEN** a caller does `GET /api/v1/sites/{id}/files/style.css`
  on a file that exists
- **THEN** the response is `200 OK` with body = file contents and
  `Content-Type: text/css`.

### Requirement: File CLI

The CLI SHALL expose `openpanel file {list,read,write,mkdir,rm,rename,chmod}`
operating on a given site by id or domain. The CLI resolves the
document root, applies the chroot check, and either prints results
(list/read) or exits 0 on success.

#### Scenario: CLI lists files

- **WHEN** the operator runs `openpanel file list --site example.com`
- **THEN** the operator sees a tabular listing of the site's document
  root with name, size, mode, and mtime.

