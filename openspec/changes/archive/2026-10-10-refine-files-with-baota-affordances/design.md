# Design: refine-files-with-baota-affordances

## Explore & Reuse (repo-map evidence, 2026-10-10)

`scripts/repo-map.sh` plus targeted reads confirm the
file-manager core already exists; this change adds web
adapters and decision-model extensions only:

- `crates/openpanel-web/src/files.rs` — current surface:
  `list`/`read`/`write`/`mkdir`/`rename`/`chmod`/`remove`/
  `upload` handlers, `listing_fragment` (Name/Type/Size/
  Mode/Modified/Actions table + footer mkdir/upload),
  `breadcrumb` (root crumb + segment links + `↑ up`),
  `editor_panel`, `error_region`, `encode_segment`/
  `encode_path`, `FileRow`, `collect_rows`, `action_response`
  re-render. Chroot is enforced in exactly one place (the
  `FilesService`); the web layer passes caller paths
  verbatim. All new affordances reuse these handlers and
  helpers; `listing_fragment` gains columns/panels, never a
  second listing renderer.
- `crates/openpanel-app` `FilesService` (chroot via
  canonicalized `document_root` + `Path::new` rejecting
  absolute/`..`/empty, atomic write, audited mutations).
  No service change: clipboard, archive, recycle, history,
  remarks are web orchestration over `list_dir`/`read_file`/
  `write_file`/`rename`/`remove`/`chmod`/`mkdir`. Recycle is
  the existing `remove(recursive: true)` scoped through the
  recoverable-delete preview, surfaced on a dedicated page.
- `crates/openpanel-web/src/ops_workflows.rs` —
  `FileSelection`/`FileAction`/`validate_file_action` (empty
  rejection, destructive-requires-confirmation),
  `is_destructive`/`is_recoverable` (Delete only),
  `ActionPreview` (scope/recoverable/requires_confirmation),
  `TaskState::banner`, `evaluate_backup_capacity`/
  `CapacityVerdict`/`capacity_banner`, `format_bytes`. The
  recycle-bin page, archive/extract progress, and the root
  free-space header extend these models (new `FileAction`
  variants Copy/Cut/Paste/Compress/Extract; clipboard +
  archive previews) rather than inventing a parallel
  workflow type.
- `crates/openpanel-web/src/router.rs:634-647` —
  `/sites/{site_id}/files*` mounting pattern (`list`,
  `read`, `write`, `mkdir`, `rename`, `chmod`, `remove`,
  `upload`, `/files` landing). New routes follow the same
  shape (`copy`, `compress`, `extract`, `recycle`,
  `history`, `favorites`, `shares`, `remarks`) with
  `WebUser` + CSRF verify + `FilesService` delegation.
- `crates/openpanel-web/src/layout.rs` — `CapabilitySet::
  shipped()` / `contains("files")`; `site_workspace.rs`
  (`workspace_tabs`, `tab_nav`, `site_bar`, `TabId::Files`)
  for preserved site context; `capability_registry.rs`
  unchanged (no new registry entry — Terminal-here links
  to the sibling `/terminal` route with a site-cwd hint).
- `crates/openpanel-web/assets/tokens.css` + `app.css` —
  shared `form` / `form-row` / `table` / `table-card` /
  `btn` / `breadcrumbs` / `banner` / `op-empty-state` /
  `op-error-state` vocabulary. All panels use `form`
  classes with paired `<label>`s, token-only values, and
  mobile-first `min-width` responsive rules; no new CSS
  families, no literal hex, no `max-width` queries.

New code is limited to `files.rs` template/handler
extensions plus `ops_workflows.rs` model extensions
(clipboard/archive/recycle/history/remarks previews).
No new domain aggregate, no new app service, no new
audit action family, no new storage backend.

## DDD layering

- **Domain** (`openpanel-domain`): untouched. `Path` /
  `FileInfo` / `FileError` invariants unchanged; history
  versions and remarks are web view projections, not new
  aggregates, so the I/O-free invariant is unaffected.
- **App** (`openpanel-app`): reused, not extended. No use
  case, repository, or migration is added.
- **Adapters** (`openpanel-web`): the only layer that
  changes. Handlers resolve `WebUser`, verify CSRF, call
  `FilesService`, render maud with shared tokens. Role
  gating is defense in depth: site tabs hide per
  `workspace_tabs`, handlers return 403/404 without
  leaking paths.

## Page flow

1. `GET /sites/{id}/files` — free-space header
   (`capacity_banner`), favorites + share-list panels,
   full breadcrumb, remarks column; clipboard toolbar
   (copy/cut/paste + shortcuts) and archive actions reuse
   `validate_file_action` previews with confirmation.
2. Compress/uncompress — `TaskState::banner` progress/
   result; blocked-capacity renders the actionable
   message instead of failing mid-flight.
3. Delete → recycle-bin page (`GET .../files/recycle`)
   listing recoverable entries with restore/empty;
   permission/owner editor with apply-to-subdirectory
   checkbox; history panel (`GET .../files/history`)
   with version rows and restore.
4. Terminal-here — anchor to `/terminal` with site-cwd
   hint; no transport, no classification change.

Sibling non-overlap: `add-migration-wizard-ui` owns
`/migrate` + `ConflictPolicy`/verifiers/cutover;
`add-terminal-nav-and-saved-commands` owns `/terminal`
+ saved servers/commands + `classify_command`;
`add-ftp-global-and-phpmyadmin-link` owns `/ftp` +
database row actions + phpMyAdmin SSO. This change
touches none of those routes or registry entries.

---

*This design AWAITS human principal approval per Agents.md §8
item 10 — do NOT implement until approved.*
