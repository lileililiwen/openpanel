# Proposal: Resolve unstyled UI classes

## Why

`assets/app.css` defines 127 class tokens; 87 of the 193 class
tokens used in `crates/openpanel-web/src/` are **never defined**.
These pages therefore render with browser defaults for the affected
class: no padding, no margin, no border, no colour, no hover. The
worst visible offenders:

- **`.site-tabs` / `.site-tab` / `.site-tab--active`** — the primary
  tab bar on every site page renders as four adjacent `<a>` with no
  active indicator (`site_workspace.rs:216-225`).
- **`.audit-summary`, `.audit-stat*`, `.audit-table`, `.audit-events`,
  `.audit-intro`, `.audit-meta*`** — the audit activity page has no
  visual hierarchy at all.
- **`.status-*`, `.status-page-*`, `.status-bars`, `.status-entry`** —
  the public status page renders as plain prose.
- **`.quick-actions`, `.attention-queue`, `.attn-*`, `.action-grid`,
  `.resource-grid`, `.server-identity`, `.disk-capacity`,
  `.network-table`, `.trend-cpu`, `.sparkline`, `.gauge-grid`,
  `.gauge-load`, `.dashboard-header`, `.dashboard-refresh`** — the
  operations dashboard has no grid, no status pills, no gauges.
- **`.breadcrumb`** (singular) — silently mismatches the defined
  `.breadcrumbs` (plural) and renders unstyled.
- **`.checkbox`** — `form label` is `flex-direction: column`, so the
  box and its text stack vertically instead of sitting inline
  (`ftp.rs:140`).
- **`.form-actions`** (14×), **`.field`** (6×) — used but
  unstyled; rely on the form baseline only by accident.
- **6 dynamic class families** that are concatenated in
  `format!("...{x}")` patterns and therefore invisible to the static
  class-extraction grep: `audit-row`, `audit-badge-*`, `audit-outcome-*`,
  `gauge-status-*`, `disk-status`, `op-status-fresh`,
  `op-status-stale`, `status-*`.

The class-extraction check (`grep -rhoE 'class="[^"]*"'`) misses
dynamic families entirely; only the dynamic lookup at render time
would expose them, and CI does not run one.

The reason this regression lasted is the missing CI gate — that gate
ships in change 3. This change ships the *content* of the fix.

## What Changes

- Define every unstyled class token in `app.css`, grouped by the
  subsystem it belongs to (audit, status page, dashboard, site
  workspace tabs, settings, marketplace, etc.). All values source
  `var(--op-*)` from `tokens.css`.
- Define the 6 dynamic class families: `audit-row` and
  `audit-row audit-outcome-ok|fail|warn`,
  `audit-badge audit-badge-ok|fail|warn`, `gauge-status-*`,
  `disk-status`, `op-status-fresh|stale`, `status-ok|warn|critical`.
- De-duplicate the two `.card` declarations: delete the
  storefront-specific one and namespace the storefront card as
  `.storefront__card` so the global `.card` keeps its dashboard
  meaning.
- Fix the `.btn` / `.button` collision: pick `.btn` as the public
  token (the existing system), turn `.button` (and the
  `.button--ghost` / `.button--danger` modifiers) into an alias
  block that reuses the same declarations.
- Fix the empty-state vocabulary: keep `.op-empty-state` (it has a
  class, it has padding, it is the public vocabulary), retire
  `.empty` and `.empty-state` to the same look via an alias block.
- Add a `label.checkbox` rule (`form label.checkbox`) that overrides
  the global `form label` `flex-direction: column` so the checkbox
  and its text sit inline.
- Replace every `class="breadcrumb"` with `class="breadcrumbs"` in
  the source (or align the two — there is one occurrence to fix).

## Capabilities

### Modified Capabilities

- `web-ui-styling`: a new requirement **All Used Classes Are
  Defined** pins the class-coverage contract. A second new
  requirement **Dynamic Class Families Are Covered** pins the
  dynamic-prefix families.

## Impact

Affects: `crates/openpanel-web/assets/app.css`,
`crates/openpanel-web/src/ftp.rs`, `crates/openpanel-web/src/web_ui_styling.rs`,
`tests/integration/web_ui_styling.rs`.

No domain / app / API changes. No new dependencies. The change
ships entirely in the web crate.
