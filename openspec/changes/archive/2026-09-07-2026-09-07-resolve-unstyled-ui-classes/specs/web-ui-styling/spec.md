# Spec delta: web-ui-styling

## ADDED Requirements

### Requirement: All Used Classes Are Defined

Every class token that appears in a `class="..."` attribute literal
inside `crates/openpanel-web/src/` MUST have a matching rule in
`crates/openpanel-web/assets/app.css`. A new class that ships in a
PR without a rule fails the build. The test enumerates literal
class tokens via static `grep` and asserts each one appears as a
rule selector in the stylesheet.

#### Scenario: Audit page classes are all defined

- **WHEN** `audit.rs` renders `.audit-summary`, `.audit-stat`,
        `.audit-stat-value`, `.audit-stat-label`, `.audit-table`,
        `.audit-events`, `.audit-intro`, `.audit-empty-note`,
        `.audit-older`, `.audit-meta`, `.audit-meta-row`,
        `.audit-meta-key`, `.audit-meta-val`, `.audit-meta-none`
- **THEN** each of those tokens has a rule in `app.css`; a new
        PR that adds an undefined class fails the static contract
        test with the token name and the file.

#### Scenario: Site workspace tabs are defined

- **WHEN** `site_workspace.rs` renders `.site-tabs`, `.site-tab`,
        `.site-tab--active`
- **THEN** the active tab is visibly distinguished from the
        inactive ones, the tab strip has padding and gap, and the
        route walk in `tests/integration/web_ui_styling.rs` does
        not flag any of the three classes as undefined.

#### Scenario: Dashboard classes are defined

- **WHEN** the operations dashboard renders `.quick-actions`,
        `.attention-queue`, `.attn-action`, `.attn-detail`,
        `.attn-label`, `.action-grid`, `.resource-grid`,
        `.resource-links`, `.server-identity`, `.server-meta`,
        `.installation-info`, `.disk-capacity`, `.network-table`,
        `.network-throughput`, `.trend-cpu`, `.sparkline`,
        `.gauge-grid`, `.gauge-load`, `.last-updated`,
        `.stale-flag`
- **THEN** each has a rule; the page has visible structure (grid
        layout, action buttons, status pills, gauges) instead of
        bare adjacent text.

#### Scenario: Status page classes are defined

- **WHEN** the public status page and the admin status page
        render `.status-public`, `.status-page-policy`,
        `.status-page-publish`, `.status-page-entries`,
        `.status-empty`, `.status-bars`, `.status-entry`,
        `.status-label`, `.status-footer`, `.status-last-ran`
- **THEN** each has a rule; the public page renders the
        expected "operational / degraded / outage" entries with
        the corresponding status colour.

#### Scenario: Singletons are defined or aliased

- **WHEN** the source emits `.form-actions` (14×), `.field`
        (6×), `.checkbox`, `.banner--warning`, `.breadcrumb`,
        `.intro`, `.muted`, `.notice`, `.ok`, `.role`, `.sep`,
        `.settings`, `.source`, `.kv`, `.host-fleet`,
        `.collaborators`, `.marketplace`, `.registry`
- **THEN** each has a rule (or an alias block, e.g. `.breadcrumb`
        → `.breadcrumbs`, `.button` → `.btn`) and the static
        contract test passes.

### Requirement: Dynamic Class Families Are Covered

Class families that are concatenated at render time via
`format!("class-{}", variant)` MUST be defined as a family in
`app.css`. The covered families are:

- `audit-row` (base) and `audit-row audit-outcome-ok|fail|warn`.
- `audit-badge` (base) and `audit-badge audit-badge-ok|fail|warn`.
- `gauge-status-ok|warn|crit`.
- `disk-status-ok|warn|crit`.
- `op-status-fresh`, `op-status-stale`.
- `status-ok`, `status-warn`, `status-critical`.

A new variant added without a rule is a regression. The static
contract test expands each family to its concrete variants via a
fixture table and asserts each concrete selector is defined.

#### Scenario: Audit outcome variants are defined

- **WHEN** `audit.rs` renders `class="audit-row audit-outcome-ok"`
        for a successful event and
        `class="audit-row audit-outcome-fail"` for a failure
- **THEN** both rules exist; the row's left border or background
        visibly differs between success and failure.

#### Scenario: Status variants are defined

- **WHEN** the dashboard renders `class="status-ok"`,
        `class="status-warn"`, `class="status-critical"`
- **THEN** each has a rule that uses the corresponding
        `--op-color-success`, `--op-color-warning`,
        `--op-color-danger` token.

#### Scenario: Stale-flag family is defined

- **WHEN** a page renders `class="op-status-stale"`
- **THEN** the rule exists and the element has a visible
        stale-state visual (e.g. a muted background, a warning
        border, or a "stale" label).

### Requirement: No Duplicated Class Declarations

No class token MUST be declared more than once in `app.css` with
contradictory properties. A new class that conflicts with an
existing one MUST fail the build. The static contract test scans
for duplicate selectors and fails if any are found.

#### Scenario: `.card` is declared exactly once

- **WHEN** the dashboard renders `.card` and the storefront
        renders `.storefront__card`
- **THEN** `app.css` has exactly one `.card` rule and exactly
        one `.storefront__card` rule; the storefront does not
        shadow the dashboard.

#### Scenario: `.btn` and `.button` are distinct

- **WHEN** the audit / notification pages use `.btn` and the
        storefront uses `.button`
- **THEN** both classes render with the same visual: same
        padding, radius, border, font-weight, hover state. The
        `.button` rule is an alias for `.btn` so a future
        styling edit cannot drift them apart silently.

### Requirement: Checkbox Labels Render Inline

`<label class="checkbox">` MUST render its checkbox and its text
inline, with the box on the left and the text on the right, with a
tokenised gap. The default `form label` rule is
`flex-direction: column`; `form label.checkbox` overrides that.

#### Scenario: Read-only checkbox on the FTP form

- **WHEN** `ftp.rs` renders
        `label class="checkbox" { input type="checkbox"; " Read only" }`
- **THEN** the checkbox and the "Read only" text sit on the
        same row, separated by `var(--op-space-2)`.
