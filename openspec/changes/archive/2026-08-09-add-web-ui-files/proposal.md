# Add Web UI — Files

## Why

Site owners need to manage their document roots: upload an index.html, edit a
config, create folders, fix permissions. The files bounded context already
implements a chrooted file manager (list/read/write/mkdir/rename/chmod/
remove with canonicalize-per-request traversal protection). The CLI covers
it; this change puts it in the browser.

## What Changes

- `crates/openpanel-web/src/files.rs`:
  - `GET /sites/{site_id}/files` — directory listing for the site's
    document root (breadcrumb path, folders + files, sizes, mtimes).
  - `GET /sites/{site_id}/files?path=...` — subdirectory navigation
    (chrooted to the site's root).
  - `GET /sites/{site_id}/files/read?path=...` — file contents (text,
    50 MB cap per the service).
  - `POST /sites/{site_id}/files/write` — save text contents.
  - `POST /sites/{site_id}/files/upload` — multipart upload.
  - `POST /sites/{site_id}/files/mkdir` — create directory.
  - `POST /sites/{site_id}/files/rename` — rename file/dir.
  - `POST /sites/{site_id}/files/chmod` — change mode.
  - `DELETE /sites/{site_id}/files/remove?path=...` — delete.
- All operations go through the existing chrooted `FilesService`; the web
  layer adds no path handling of its own beyond passing the caller-supplied
  path to the service.

## Non-Goals

- Image preview / media players — text read + download links only.
- A full drag-and-drop manager — HTML upload input with HTMX submit.
- Multi-select / bulk operations.

## Capabilities

### Existing Capabilities

- `web-ui`: adds the file manager pages to the shell.
- `files`: consumed through the `FilesService` (chroot enforced there).
- `sites`: files pages are mounted per site (`/sites/{site_id}/files`).
