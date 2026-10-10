# Proposal: refine-files-with-baota-affordances

## WHY

Operators coming from Baota expect a dense file-manager
surface: favorites, share list, Terminal-here, free-space
header, copy/cut/paste with shortcuts, compress/uncompress,
recycle bin, recursive permission/owner, version history,
remarks, and full breadcrumb navigation. OpenPanel ships only
list/read/write/mkdir/rename/chmod/remove/upload
(`crates/openpanel-web/src/files.rs`) with a root-crumb plus
`↑ up` link, per-row rename/chmod/delete, and footer
mkdir/upload forms. The decision models that should govern
the new affordances already exist
(`crates/openpanel-web/src/ops_workflows.rs`:
`validate_file_action`, `FileAction::is_recoverable`,
`evaluate_backup_capacity`, `TaskState::banner`), but no
browser surface consumes them for files. Operators therefore
drop to raw paths where chroot, confirmation, and
recoverability expectations are invisible.

## WHAT

Refine the site-scoped file manager (`/sites/{id}/files`)
with Baota-parity affordances rendered through existing
services: favorites + share-list panels (scoped visibility
only), Terminal-here deep-link (hint only, no transport),
root free-space header reusing the capacity verdict,
clipboard copy/cut/paste with keyboard shortcuts, compress/
uncompress actions, recycle-bin page backed by the
recoverable-delete model, permission/owner editor with
apply-to-subdirectory, file-history versions panel, remarks
column with inline edit, and full segmented breadcrumb nav.
Every mutation reuses `FilesService` chroot enforcement and
per-session CSRF; destructive actions reuse the confirmation
+ recoverable preview.

## Capabilities

- **Modified:** `files` — listing affordances, clipboard,
  archive, recycle, permission/owner, history, remarks,
  breadcrumb deltas (this change).
- **Modified:** `operations-workflows` — file-action preview,
  recycle-bin workflow, capacity header, progress/result
  states for archive/extract (deltas in this change).
- Reused unchanged: `capability-navigation` (no new
  registry entry), `web-ui-styling` (token vocabulary),
  identity/auth + sites RBAC, `terminal-host-fleet`
  (link target only).

## Non-goals

- No chroot change; enforcement stays in `FilesService`,
  web hands paths through verbatim.
- No new storage backend, filesystem driver, or quota
  engine; free space is a read projection.
- No sharing ACL beyond scoped visibility; no public
  links, no cross-site shares.
- No terminal transport or PTY (sibling
  `add-terminal-nav-and-saved-commands` owns `/terminal`).
- No migration-wizard overlap (see sibling
  `add-migration-wizard-ui`) and no FTP/database row
  overlap (see sibling
  `add-ftp-global-and-phpmyadmin-link`).
