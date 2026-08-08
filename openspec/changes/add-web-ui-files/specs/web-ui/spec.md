## ADDED Requirements

### Requirement: File Manager Navigation

The `web-ui` SHALL render a per-site file manager at
`GET /sites/{site_id}/files` that lists the site's document root inside the
shell: a breadcrumb path, and one row per entry (name, type, size, modified
time). Navigating into a subdirectory SHALL request `?path=<prefix>` and
SHALL be confined to the site's document root by the existing chrooted
`FilesService`.

#### Scenario: Listing a directory

- **WHEN** an authorized user opens `/sites/{id}/files` with a path inside
  the site's document root
- **THEN** the server renders the breadcrumb and the directory entries.

#### Scenario: Escaping the site root is rejected

- **WHEN** a path resolves outside the site's document root
- **THEN** the handler shows the service's inline error and renders no
  entries.

### Requirement: File Read and Write

The UI SHALL provide `GET .../files/read` (renders text contents in an editor
or a download link for binary/large files) and `POST .../files/write` (saves
contents). Reads SHALL honor the service's size cap.

#### Scenario: Editing a text file

- **WHEN** an authorized user reads a text file and saves edits
- **THEN** the written contents are persisted within the site root and the
  editor re-renders the saved contents.

### Requirement: File Mutations

The UI SHALL support upload, mkdir, rename, chmod, and delete for entries in
the site root, each through the service with CSRF validation. Delete SHALL
require a confirmation.

#### Scenario: Creating and removing entries

- **WHEN** an authorized user uploads, creates, renames, or removes entries
- **THEN** the listing re-renders reflecting the change.

#### Scenario: Confirmed delete

- **WHEN** an authorized user confirms deletion of an entry
- **THEN** the entry is removed and the listing re-renders without it.
