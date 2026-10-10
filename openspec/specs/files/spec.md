# files Specification

## Purpose

Sites and databases ship in v0.1-alpha. Operators can provision vhosts and MySQL databases but cannot upload the actual site contents. To deploy a WordPress theme, a static site, or any web content, the operator needs a file manager scoped to a site's document root. This change adds the `files` bounded context: a chrooted file manager for sites. Every operation is restricted to the site's document root (no path traversal), and every mutation is audited.
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

### Requirement: File Favorites Are Scoped Shortcuts

The file manager MUST offer per-site favorites: pinned
site-relative paths rendered above the listing with jump
and unpin actions. Favorites MUST be scoped to the current
site chroot; absolute paths and cross-site targets MUST
never render or resolve.

#### Scenario: Favorites render scoped entries

- **WHEN** an authorized user pins `wp-content/themes`
  and opens the file manager
- **THEN** the favorites panel shows the pinned entry
  linking inside the same site root, and unpin removes
  it without touching the filesystem.

### Requirement: Share List Is Visibility Only

The file manager MUST render a share list of scoped
entries visible to the caller's collaborators. Sharing
MUST NOT create public links, cross-site entries, or ACL
semantics beyond scoped visibility; every target MUST pass
the `FilesService` chroot check.

#### Scenario: Share list shows scoped entries

- **WHEN** an authorized user opens the share list
- **THEN** only same-site, chroot-passing entries render,
  with no public URL and no cross-site target.

### Requirement: Terminal-Here Is A Deep Link Only

The file manager MUST render a Terminal-here action that
links to the sibling `/terminal` route with a site-cwd
hint. It MUST NOT add terminal transport, PTY streaming,
or classification semantics.

#### Scenario: Terminal-here links out

- **WHEN** an authorized user clicks Terminal-here from
  `wp-content`
- **THEN** the anchor resolves to the mounted `/terminal`
  route carrying the site-cwd hint, with no new endpoint
  mounted by this change.

### Requirement: Root Shows Free-Space Header

The file listing MUST render a free-space header at the
site root derived from `evaluate_backup_capacity`: the
estimated selection size against destination free space,
with blocked-capacity rendering the actionable message.

#### Scenario: Over-capacity header blocks

- **WHEN** the estimated archive exceeds free space
- **THEN** the header renders the blocked message and the
  archive action is refused before any mutation.

### Requirement: Clipboard Copy Cut Paste With Shortcuts

The file manager MUST support clipboard copy/cut/paste
over the current selection with keyboard shortcuts
(Ctrl/Cmd+C, X, V). Empty-clipboard paste MUST be
rejected; destructive paste MUST require confirmation via
`validate_file_action` preview.

#### Scenario: Paste without confirmation is refused

- **WHEN** a cut selection is pasted without confirmation
- **THEN** the handler refuses with the confirmation
  reason and mutates nothing.

### Requirement: Compress And Uncompress With Progress

The file manager MUST offer compress (archive selection)
and uncompress (extract archive) actions rendering
`TaskState::banner` progress/result states. Both MUST
stay inside the chroot and reuse the capacity verdict.

#### Scenario: Extract renders progress then result

- **WHEN** an authorized user extracts a zip inside the
  site root
- **THEN** the UI shows running progress and then success
  or a secret-free failure, with entries inside the root.

### Requirement: Recycle Bin Page Is Recoverable

The file manager MUST expose a recycle-bin page listing
recoverably deleted entries with restore and empty
actions. Delete MUST route through the recoverable
preview (`is_recoverable`) with confirmation.

#### Scenario: Deleted entry is restorable

- **WHEN** an authorized user confirms deletion of
  `old.css`
- **THEN** the entry appears in the recycle bin with a
  restore action, and empty requires a second
  confirmation.

### Requirement: Permission Owner Editor Applies To Subdirectory

The permission/owner editor MUST accept octal mode plus
owner/group with an apply-to-subdirectory checkbox. Bad
octal MUST be rejected inline; recursive apply MUST stay
inside the chroot and audit each mutation.

#### Scenario: Bad mode is rejected inline

- **WHEN** an authorized user submits mode `999`
- **THEN** the form re-renders a `form-error` and changes
  nothing.

### Requirement: File History Versions Are Restorable

The file manager MUST render a per-file history panel of
versions (mtime, size, author) with a restore action.
History MUST NOT leak file contents in list views.

#### Scenario: Version restore is audited

- **WHEN** an authorized user restores version 3 of
  `index.html`
- **THEN** the file contents revert to that version and
  an audit row records path plus version, not contents.

### Requirement: Remarks Column Is Inline Editable

The listing MUST include a remarks column with an inline
edit form per row. Every input MUST carry a paired label
and token-only styling.

#### Scenario: Remark edit saves inline

- **WHEN** an authorized user saves a remark on `app.js`
- **THEN** the row re-renders the new remark text with
  its label intact and no page-level error.

### Requirement: Breadcrumb Navigates Every Segment

The listing MUST render a full segmented breadcrumb
(site root through every ancestor to current) with each
ancestor linked and current as text, plus the `↑ up`
parent link. All segments MUST be URL-encoded per the
existing `encode_path` rule.

#### Scenario: Deep path crumbs link correctly

- **WHEN** an authorized user opens `a/b/c`
- **THEN** root, `a`, and `a/b` render as links, `c`
  renders as current text, and special characters are
  percent-encoded.

