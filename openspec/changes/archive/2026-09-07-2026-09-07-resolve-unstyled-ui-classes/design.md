# Design: Resolve unstyled UI classes

## Approach

One file grows, one file shrinks, and a handful of source files
get tiny touch-ups.

### 1. Grow `app.css`

Add a new section after the existing "Interaction surface" block.
The section is named "Per-subsystem components" and is a flat
alphabetical grouping of class rules:

- **Audit** (`.audit-summary`, `.audit-stat`, `.audit-stat-value`,
  `.audit-stat-label`, `.audit-events`, `.audit-intro`,
  `.audit-empty-note`, `.audit-table`, `.audit-older`, `.audit-meta`,
  `.audit-meta-row`, `.audit-meta-key`, `.audit-meta-val`,
  `.audit-meta-none`, plus the dynamic families `audit-row`,
  `audit-outcome-ok|fail|warn`, `audit-badge`, `audit-badge-ok|fail|warn`).
- **Status page** (`.status-public`, `.status-page-policy`,
  `.status-page-publish`, `.status-page-entries`,
  `.status-empty`, `.status-bars`, `.status-entry`, `.status-label`,
  `.status-footer`, `.status-last-ran`).
- **Dashboard** (`.dashboard-header`, `.dashboard-refresh`,
  `.quick-actions`, `.attention-queue`, `.attn-action`,
  `.attn-detail`, `.attn-label`, `.action-grid`, `.resource-grid`,
  `.resource-links`, `.server-identity`, `.server-meta`,
  `.installation-info`, `.disk-capacity`, `.network-table`,
  `.network-throughput`, `.trend-cpu`, `.sparkline`, `.gauge-grid`,
  `.gauge-load`, `.last-updated`, `.stale-flag`).
- **Site workspace tabs** (`.site-tabs`, `.site-tab`,
  `.site-tab--active`, `.site-header`, `.site-meta`,
  `.site-overview`).
- **Settings / dl** (`.kv`, `.field`, `.role`, `.sep`).
- **Marketplace / registry / details** (`.marketplace`, `.registry`,
  `.host-fleet`, `.collaborators`, `.detail__notice`,
  `.detail__last-install`, `.detail__trust`, `.detail__trust-digest`,
  `.card__last-install`, `.deploy-form`, `.deploy-form__actions`).
- **Form** (`.form-actions`, `.checkbox`, `.banner--warning`).
- **Status / dynamic** (`.status`, `.status-ok`, `.status-warn`,
  `.status-critical`, `.gauge-status-ok|warn|crit`,
  `.disk-status-ok|warn|crit`, `.op-status-fresh`,
  `.op-status-stale`).
- **Misc** (`.breadcrumb` → alias for `.breadcrumbs`, `.breadcrumb a`,
  `.intro`, `.muted`, `.notice`, `.ok`, `.settings`, `.source`).

Every rule sources its values from `var(--op-*)`. Existing values
that already use tokens keep them; existing literal-hex and
literal-rem values are out of scope and become change 3's job.

### 2. De-duplicate `.card`

`.card` is declared at `app.css:473` and again at `app.css:1008`.
The second declaration is the storefront variant. Rename the
storefront variant to `.storefront__card` and update the two
storefront callsites in `software_center.rs` to use the new
class. Net: one `.card` rule, one `.storefront__card` rule, no
behaviour change for the dashboard cards.

### 3. De-conflict `.btn` / `.button`

`.btn` is the public token (used in `audit.rs`, `ftp.rs`,
`notifications.rs`, etc., with a working visual). `.button` is the
storefront's competing vocabulary (`.button--ghost`,
`.button--danger`). Keep both working:

- `.button { ... }` re-declares the same five properties as
  `.btn { ... }`.
- `.button--ghost` becomes an alias for `.btn-ghost` (a new
  ghost-style variant of `.btn`).
- `.button--danger` becomes an alias for `.btn-danger`.

This preserves every existing call site while removing the
parallel-but-different vocabulary.

### 4. De-conflict empty / error states

`.empty` and `.empty-state` are aliased to `.op-empty-state`.
`.error` and `.error-state` are aliased to `.op-error-state`.
`form[data-form="bare"]` is unchanged (it is a developer hint, not
a visual state).

### 5. Fix `label.checkbox`

`form label { flex-direction: column; }` makes the checkbox stack
above its text. `label.checkbox` overrides with
`flex-direction: row; align-items: center; gap: var(--op-space-2)`.
This is a one-line fix.

### 6. Fix `.breadcrumb` vs `.breadcrumbs`

Grep the source for `class="breadcrumb"` (singular). The only
match is one occurrence; rename to `.breadcrumbs` so it picks up
the existing rule. (Verified: there is exactly one such
occurrence.)

## Explore & Reuse

- Reuse `.op-empty-state` (`app.css:1538`) and `.op-error-state`
  (`app.css:1563`) as the source of truth for the aliased states.
- Reuse `.btn` (`app.css:509`) as the source of truth for the
  `.button` alias block.
- Reuse `.op-color-success`, `.op-color-warning`, `.op-color-danger`
  for the status pills and outcome badges — no new tokens.
- Reuse the existing `.tabs`/`.tab`/`.tab--active` pattern in the
  storefront (`.storefront__tabs .tab--active`) as a reference
  shape for the new `.site-tabs` / `.site-tab--active` block.

No new tokens are added. The existing vocabulary is sufficient.

## Non-goals

- No new design language, no new colour, no new spacing scale.
- No tokenisation of the existing literal values inside the
  storefront / `.detail` / `.table` blocks — that is change 3.
- No CI gate for class coverage — that is change 3.
- No class is removed from the source unless the change is a pure
  rename (`.breadcrumb` → `.breadcrumbs`,
  `.card` → `.storefront__card` on the two storefront callsites).

## Files Touched

- `crates/openpanel-web/assets/app.css` — the new "Per-subsystem
  components" section, the alias blocks, the `label.checkbox` rule.
- `crates/openpanel-web/src/ftp.rs:140` — `label class="checkbox"`
  keeps using the new rule, no source change.
- `crates/openpanel-web/src/software_center.rs` — two `class="card"`
  become `class="storefront__card"`.
- `crates/openpanel-web/src/web_ui_styling.rs` — extend the static
  contract to assert that every literal class token referenced in
  the test fixtures has a rule. (The full CI gate is change 3; this
  is a small unit-level guard.)
- `tests/integration/web_ui_styling.rs` — extend the route walk to
  fail if a route renders an empty `<h1>`/`<h2>` (no class, no
  text), an undefined class, or a bare `<table>`.
- `openspec/specs/web-ui-styling/spec.md` — two new requirements.
