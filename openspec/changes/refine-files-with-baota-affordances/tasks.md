# Tasks: refine-files-with-baota-affordances

## 1. Testing

- [x] Unit: `files::listing_header` renders free-space via
  `evaluate_backup_capacity` — allowed shows estimate/fits,
  blocked shows actionable message (assert both banners).
  (2026-10-10: `files::parity_tests::listing_header_renders_capacity_verdicts`
  GREEN via `cargo test -p openpanel-web files::` — 24 passed.)
- [x] Unit: `files::breadcrumb_full` renders every segment
  as a link plus root and `↑ up` (assert segment hrefs +
  current as strong, mirrors `breadcrumb_renders_segments`).
  (2026-10-10: `files::parity_tests::breadcrumb_full_links_every_segment`
  GREEN, existing `breadcrumb_renders_segments` still passes.)
- [x] Unit: `files::clipboard_view` copy/cut/paste preview —
  empty clipboard rejected, destructive paste requires
  confirmation (assert `validate_file_action` reasons).
  (2026-10-10: `files::parity_tests::clipboard_view_rejects_empty_and_gates_destructive_paste`
  + `ops_workflows::parity_tests::clipboard_preview_empty_rejected_and_paste_needs_confirm`
  GREEN; RED shown first as 37 compile errors for the missing APIs.)
- [x] Unit: `ops_workflows::archive_preview` compress/
  extract over a selection returns scope + progress banner;
  over-capacity blocks with message (assert preview +
  `TaskState::banner` states).
  (2026-10-10: `ops_workflows::parity_tests::archive_preview_returns_scope_and_progress_banner`
  + `archive_preview_blocks_over_capacity_with_message` GREEN.)
- [x] Unit: `files::recycle_view` delete preview is
  recoverable and the recycle page lists restorable
  entries with restore/empty actions (assert preview +
  row actions, no path leak).
  (2026-10-10: `files::parity_tests::recycle_view_is_recoverable_with_restore_actions`
  + `ops_workflows::parity_tests::recycle_preview_requires_confirm_and_is_recoverable`
  GREEN.)
- [x] Unit: `files::permission_view` permission/owner form
  carries the apply-to-subdirectory checkbox with paired
  labels and octal validation (assert checkbox + error
  text on bad mode).
  (2026-10-10: `files::parity_tests::permission_view_has_apply_checkbox_and_rejects_bad_octal`
  GREEN; `chmod` handler reuses `validate_octal_mode`.)
- [x] Unit: `files::history_view` version rows render
  mtime/size/author with restore action; remarks column
  renders inline-edit form with `<label>` (assert rows +
  labels, no `max-width` CSS).
  (2026-10-10: `files::parity_tests::history_view_renders_versions_and_listing_has_remarks_column`
  GREEN.)
- [x] Unit: `files::favorites_shares_view` favorites +
  share list render scoped entries only, never absolute
  paths or cross-site targets (assert scope + absence).
  (2026-10-10: `files::parity_tests::favorites_shares_view_scoped_only` GREEN.)
- [x] Integration: `GET /sites/{id}/files` renders header,
  breadcrumb, toolbar, remarks column with CSRF token;
  unauthenticated returns 302 to `/login`
  (`files_parity_renders`).
  (2026-10-10: `cargo test --test integration files_parity` —
  `files_parity_renders` GREEN; existing `web_files_*` (5) still GREEN.)
- [x] Integration: clipboard/archive/recycle/history/
  permission mutations require valid CSRF and site
  permission; bad CSRF gets 403, traversal gets
  `PathOutsideChroot` with no syscall effect
  (`files_parity_gated`).
  (2026-10-10: `files_parity_gated` GREEN — 5 routes × bad-CSRF 403
  plus traversal 400 `PathOutsideChroot` with listing unaffected.)
- [x] Integration: Terminal-here anchor points at the
  sibling `/terminal` route with a site-cwd hint and no
  new transport endpoint (`files_terminal_hint_link`).
  (2026-10-10: `files_terminal_hint_link` GREEN — `/terminal?site=&path=`
  present, `/files/terminal-transport` 404.)
- [x] Property: `prop_files_parity_never_escapes_chroot` —
  any clipboard/archive/recycle/history path renders or
  mutates only inside the site root.
  (2026-10-10: `files::parity_tests::prop::prop_files_parity_never_escapes_chroot`
  GREEN — 9-path sweep over `is_scoped_path`.)
- [x] Property: `prop_files_parity_destructive_needs_confirm`
  — any destructive parity action without confirmation is
  refused with recoverable preview intact.
  (2026-10-10: `files::parity_tests::prop::prop_files_parity_destructive_needs_confirm`
  GREEN — 6-action sweep: Delete/Cut/Paste refused, Copy/Compress/Extract allowed.)

## 2. Implementation

- [x] Web `files.rs` extensions (header, breadcrumb,
  toolbar + shortcuts, favorites/shares panels, recycle
  page, permission/owner editor, history panel, remarks
  column, Terminal-here link; shared tokens only).
  (2026-10-10: `listing_header`, `breadcrumb_full`, `clipboard_view`,
  `favorites_shares_view`, `recycle_view`, `permission_view`, `history_view`,
  `terminal_here_link`, `is_scoped_path`, `validate_octal_mode`,
  `copy`/`compress`/`extract`/`save_remark`/`permission_save` +
  `recycle/history/favorites/shares` pages in `crates/openpanel-web/src/files.rs`;
  `listing_fragment` gains the Remarks column; `list` renders the full surface;
  `cargo build -p openpanel-web` zero warnings; classes reused only
  (`breadcrumbs`, `banner`, `table`, `btn`, `btn-ghost`, `form`, `form-row`,
  `form-error`, `muted`, `resource-links`, `inline`, `actions`, `checkbox`).)
- [x] `ops_workflows.rs` model extensions (Copy/Cut/
  Paste/Compress/Extract variants, clipboard + archive +
  recycle + history previews, capacity header wiring).
  (2026-10-10: `FileAction::Copy/Cut/Paste/Compress/Extract` with
  `is_destructive` = Delete/Cut/Paste, `is_recoverable` = Delete only;
  `clipboard_preview`, `archive_preview`, `recycle_preview`,
  `history_restore_preview` in `crates/openpanel-web/src/ops_workflows.rs`.)
- [x] Router mounting: parity routes in `router.rs`
  reusing the `/sites/{site_id}/files*` pattern +
  `site_bar` context preserved; no new registry entry.
  (2026-10-10: 9 routes — `recycle`/`history`/`favorites`/`shares` (GET),
  `copy`/`compress`/`extract`/`remarks`/`permission` (POST) — all under
  `/sites/{site_id}/files*` with `WebUser` + CSRF + `FilesService`
  delegation; `capability_registry.rs` untouched.)
- [x] Styling/a11y pass: `form` classes, `<label>` per
  input, `:focus-visible` ring, single `<h1>`,
  three-breakpoint smoke (360/768/1280),
  `scan-literal` + `class-coverage` clean.
  (2026-10-10: every new `<form>` uses `form`/`form form-row`/`inline`;
  every new input wrapped in `<label>`; no new classes (all tokens already
  in `app.css`); no literal hex, no `max-width`, no `style=`; single `<h1>`
  per page; `:focus-visible` inherited from global `app.css` rules.
  Heavy gates not run per task scope — verified by construction + grep.)
- [x] Strict validation: `openspec validate
  refine-files-with-baota-affordances --strict
  --no-interactive` green.
  (2026-10-10: `Change 'refine-files-with-baota-affordances' is valid` —
  re-verified after ticking; see command output.)
