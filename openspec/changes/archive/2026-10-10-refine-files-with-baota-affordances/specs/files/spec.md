# files Specification

## ADDED Requirements

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
