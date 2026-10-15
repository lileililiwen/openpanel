# webdav-file-access Specification

## Purpose

Promoted from the `adopt-openserverpanel-batch` capability set borrowed
from OpenServerPanel. Web owners and CMS workflows routinely mount a
remote folder over WebDAV (macOS Finder, Windows Map Network Drive,
Cyberduck). This capability adds a first-class `webdav` bounded
context: per-site accounts served under `/dav`, Basic authentication
against argon2-hashed credentials, chroot path resolution with no
traversal or symlink escape, read-only enforcement, and the core DAV
verb set — mirroring the containment guarantees of the FTP and file
manager modules.
## Requirements
### Requirement: WebDAV Account Lifecycle

The webdav bounded context SHALL manage accounts keyed to a site with
username, chroot home path, read-only flag, enabled flag, and argon2
password hash. Mutating account operations require an authorized
owner/admin role and produce audit events; credential material is
never returned by API or logs.

#### Scenario: Create account

- **WHEN** an authorized operator creates a WebDAV account for a site
- **THEN** the account is persisted with a hashed password and the
  plaintext is shown exactly once in the creation response.

#### Scenario: Disabled account rejected

- **WHEN** a client authenticates against a disabled account
- **THEN** the request is rejected with `401 Unauthorized` and no
  filesystem operation occurs.

### Requirement: Chroot Containment

Every WebDAV request MUST resolve inside the account's home directory;
path traversal (`..`), absolute-path smuggling, and symlink escapes
from the home MUST be rejected.

#### Scenario: Traversal rejected

- **WHEN** a request targets a path that resolves outside the home
- **THEN** the server responds `404 Not Found` and nothing outside the
  home is read or written.

### Requirement: Core Verb Set

The server SHALL implement OPTIONS, PROPFIND (Depth 0 and 1), GET,
HEAD, PUT, DELETE, MKCOL, MOVE, and COPY for authenticated accounts,
with correct `207 Multi-Status` XML for PROPFIND listings and
`Content-Length` for GET. PROPPATCH SHALL answer `207` with an empty
dead-property result. LOCK/UNLOCK and ACL are explicitly out of scope
and answer `501 Not Implemented`.

#### Scenario: Directory listing

- **WHEN** an authenticated client sends `PROPFIND Depth: 1` on the
  account home
- **THEN** the response is `207` with one XML `response` per entry
  containing displayname, collection flag, size, and modification time.

#### Scenario: Unsupported verbs answered

- **WHEN** a client sends `LOCK`
- **THEN** the server responds `501 Not Implemented`.

### Requirement: Read-Only Enforcement

Accounts flagged read-only MUST reject PUT, DELETE, MKCOL, MOVE, COPY,
and PROPPATCH with `403 Forbidden` while GET, HEAD, and PROPFIND
succeed.

#### Scenario: Read-only write attempt

- **WHEN** a read-only account issues `PUT`
- **THEN** the response is `403` and the file is unchanged.

### Requirement: Authentication Before Content

Every DAV verb MUST require valid Basic credentials before any
filesystem metadata is revealed; unauthenticated requests receive `401`
with a `WWW-Authenticate` challenge and no body.

#### Scenario: Anonymous PROPFIND

- **WHEN** an unauthenticated `PROPFIND` arrives
- **THEN** the response is `401` with `WWW-Authenticate: Basic realm="WebDAV"`.
