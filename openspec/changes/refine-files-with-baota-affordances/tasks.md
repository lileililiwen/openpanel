# Tasks: refine-files-with-baota-affordances

## 1. Testing

- [ ] Unit: `files::listing_header` renders free-space via
  `evaluate_backup_capacity` — allowed shows estimate/fits,
  blocked shows actionable message (assert both banners).
- [ ] Unit: `files::breadcrumb_full` renders every segment
  as a link plus root and `↑ up` (assert segment hrefs +
  current as strong, mirrors `breadcrumb_renders_segments`).
- [ ] Unit: `files::clipboard_view` copy/cut/paste preview —
  empty clipboard rejected, destructive paste requires
  confirmation (assert `validate_file_action` reasons).
- [ ] Unit: `ops_workflows::archive_preview` compress/
  extract over a selection returns scope + progress banner;
  over-capacity blocks with message (assert preview +
  `TaskState::banner` states).
- [ ] Unit: `files::recycle_view` delete preview is
  recoverable and the recycle page lists restorable
  entries with restore/empty actions (assert preview +
  row actions, no path leak).
- [ ] Unit: `files::permission_view` permission/owner form
  carries the apply-to-subdirectory checkbox with paired
  labels and octal validation (assert checkbox + error
  text on bad mode).
- [ ] Unit: `files::history_view` version rows render
  mtime/size/author with restore action; remarks column
  renders inline-edit form with `<label>` (assert rows +
  labels, no `max-width` CSS).
- [ ] Unit: `files::favorites_shares_view` favorites +
  share list render scoped entries only, never absolute
  paths or cross-site targets (assert scope + absence).
- [ ] Integration: `GET /sites/{id}/files` renders header,
  breadcrumb, toolbar, remarks column with CSRF token;
  unauthenticated returns 302 to `/login`
  (`files_parity_renders`).
- [ ] Integration: clipboard/archive/recycle/history/
  permission mutations require valid CSRF and site
  permission; bad CSRF gets 403, traversal gets
  `PathOutsideChroot` with no syscall effect
  (`files_parity_gated`).
- [ ] Integration: Terminal-here anchor points at the
  sibling `/terminal` route with a site-cwd hint and no
  new transport endpoint (`files_terminal_hint_link`).
- [ ] Property: `prop_files_parity_never_escapes_chroot` —
  any clipboard/archive/recycle/history path renders or
  mutates only inside the site root.
- [ ] Property: `prop_files_parity_destructive_needs_confirm`
  — any destructive parity action without confirmation is
  refused with recoverable preview intact.

## 2. Implementation

- [ ] Web `files.rs` extensions (header, breadcrumb,
  toolbar + shortcuts, favorites/shares panels, recycle
  page, permission/owner editor, history panel, remarks
  column, Terminal-here link; shared tokens only).
- [ ] `ops_workflows.rs` model extensions (Copy/Cut/
  Paste/Compress/Extract variants, clipboard + archive +
  recycle + history previews, capacity header wiring).
- [ ] Router mounting: parity routes in `router.rs`
  reusing the `/sites/{site_id}/files*` pattern +
  `site_bar` context preserved; no new registry entry.
- [ ] Styling/a11y pass: `form` classes, `<label>` per
  input, `:focus-visible` ring, single `<h1>`,
  three-breakpoint smoke (360/768/1280),
  `scan-literal` + `class-coverage` clean.
- [ ] Strict validation: `openspec validate
  refine-files-with-baota-affordances --strict
  --no-interactive` green.
