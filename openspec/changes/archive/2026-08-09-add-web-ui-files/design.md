# Design: Add Web UI — Files

## Context

`FilesService` is chrooted: every call canonicalizes the requested path and
rejects anything outside the site's document root. The web layer must not
re-implement path validation — it passes paths to the service and renders the
result. Files are mounted per site because the service is site-scoped.

## Decisions

### 1. Path handling delegated entirely to the service

**Decision**: Web handlers take `?path=` from the URL/form and hand it
verbatim to `FilesService` (`list_dir`, `read_file`, `write_file`, `mkdir`,
`remove`, `rename`, `chmod`). The service performs the canonicalize + chroot
check. The UI renders the service's error (e.g. "path escapes site root")
inline.

**Rationale**: The chroot guarantee lives in exactly one place (the service).
Duplicating path logic in the web layer would create a second, attackable
surface.

### 2. Breadcrumb navigation

**Decision**: The listing renders a breadcrumb (site root → … → current dir)
built by splitting the service-resolved path. Each segment links to the
listing with that prefix. Paths render URL-encoded in `href`s.

**Rationale**: Paths in a URL are escaped and can contain unusual characters;
encoding each segment keeps the URL valid while the service still receives the
decoded path.

### 3. Read + write as a text editor panel

**Decision**: `read` renders file contents in a `<textarea>` inside the shell;
`write` submits the textarea to the service. Binary files show a
"binary/large — download only" notice (download link served by the read
route with attachment disposition).

**Rationale**: The 50 MB cap and text-centric model match the service; an
inline editor is the highest-value interaction for site owners.

### 4. HTMX for listing mutations

**Decision**: mkdir/rename/chmod/delete/upload submit via HTMX and swap the
listing region. Delete reuses the confirmation `<dialog>` pattern from
sites/databases.

**Rationale**: Consistent with the shell's interaction model.

### 5. Upload via multipart

**Decision**: `upload` accepts `multipart/form-data` (the axum `multipart`
feature is already enabled) and renders the refreshed listing on success.

**Rationale**: Reuses the existing axum multipart stack already wired for the
API upload route.

## Security

- All state-changing routes validate CSRF.
- Paths are rendered through URL encoding + `maud` escaping; contents are
  rendered into `<textarea>`/`<pre>` (HTML-escaped), never injected.
- The chroot boundary is enforced by the service, not the web layer.
- Upload size follows the service's existing cap.

## Test strategy

- Unit: listing rendering (rows, breadcrumb segments, encode), read/write
  panel markup, binary-notice branch.
- Integration via `TestServer` (sandboxed per-site roots): list root →
  write file → read back → mkdir → rename → chmod → delete; escape attempt
  (path with `..`) rejected by the service and shown inline; CSRF mismatch
  403; unauthenticated redirect; RBAC scoping.

## Notes

- No new dependencies. Reuses `FilesService`, the shell, and axum multipart.
