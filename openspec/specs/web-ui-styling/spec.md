# web-ui-styling Specification

## Purpose
TBD - created by archiving change 2026-08-14-add-web-ui-styling-and-responsive-layout. Update Purpose after archive.
## Requirements
### Requirement: Global Form Contract

Every `<form>` element in the web UI MUST declare a class from the
global set: `form`, `form form-grid`, or `form form-row`. The class
sources every colour, spacing, and typography value from
`tokens.css`; the form MUST NOT declare a literal value, a one-off
class name, or rely on browser defaults for any descendant element.

#### Scenario: Form has the global class

- **WHEN** any web route renders a `<form>`
- **THEN** the form declares `class="form"` (or `form form-grid`,
        or `form form-row`) and no descendant declares a literal
        colour or a non-token spacing value.

#### Scenario: Form has no unstyled inputs

- **WHEN** a developer adds a `<form>` without the global class
- **THEN** `tests/integration/web_ui_styling.rs` fails the build
        with the offending file and the missing class.

### Requirement: Paired Labels

Every `<input>`, `<textarea>`, and `<select>` MUST be paired with a
`<label>` either via a wrapping `<label>` or an explicit
`for`/`id` association. The visual focus ring MUST be visible via
`:focus-visible`.

#### Scenario: Input has a label

- **WHEN** the rendering pipeline encounters an input
- **THEN** a sibling or parent `<label>` exists with text content;
        the `:focus-visible` rule paints a 2 px ring in
        `var(--op-color-focus-ring)`.

### Requirement: Three-Breakpoint Responsive Layout

Every public web route MUST render without horizontal overflow at
the three breakpoints: 360 px (mobile), 768 px (tablet), and 1280 px
(desktop). The CSS uses the mobile-first `min-width` pattern;
`max-width` media queries are forbidden.

#### Scenario: Mobile viewport fits

- **WHEN** a page is rendered at 360 px
- **THEN** `<body>` has zero horizontal overflow and every form
        control is at least 44 px tall (touch target floor).

#### Scenario: Desktop viewport uses full width

- **WHEN** a page is rendered at 1280 px
- **THEN** the layout uses the wider sidebar and the form max-width
        matches the design tokens (480 / 720 px).

### Requirement: Token-Only Styling

Every CSS rule outside `tokens.css` MUST source colours, spacing,
radii, and typography from the token custom properties declared in
`tokens.css`. Literal hex codes, pixel-spacing values, or non-token
font families are forbidden outside `tokens.css`.

#### Scenario: CSS uses tokens

- **WHEN** the quality scan runs over `app.css`
- **THEN** the scan reports zero literal hex colours, zero
        `@media (max-width: …)` queries, and zero non-token pixel
        spacing values; CI fails otherwise.

### Requirement: Tables Scroll Inside a Card

Wide tables MUST be wrapped in a `.table-card` container that adds
horizontal overflow scrolling. The page itself MUST NOT scroll
horizontally on a narrow viewport.

#### Scenario: Narrow viewport, wide table

- **WHEN** a table wider than the viewport renders
- **THEN** the table fits inside a `.table-card` with
        `overflow-x: auto`; the page body remains within the
        viewport width.

### Requirement: Filter Forms Are In Contract Scope

Filter forms — a `<form method="get">` that narrows a listing and carries
no CSRF token — SHALL be covered by the Global Form Contract and Paired
Labels requirements, and SHALL be walked by
`tests/integration/web_ui_styling.rs` like any other form.

#### Scenario: A filter form declares an ad-hoc class

- **WHEN** a filter form declares a one-off class token such as
  `audit-filters` alongside the global `form` class
- **THEN** `tests/integration/web_ui_styling.rs` fails and names the
  route and the offending token.

#### Scenario: A filter control has no paired label

- **WHEN** a visible input, select, or textarea in a filter form has
  neither a wrapping `<label>` nor a matching `<label for="...">`
- **THEN** `tests/integration/web_ui_styling.rs` fails and names the
  route and the control.

#### Scenario: A placeholder merely repeats its label

- **WHEN** a control's `placeholder` text is identical to the text of
  its paired label
- **THEN** the placeholder is dropped so the text is not rendered twice.

### Requirement: Global Element Baseline

`crates/openpanel-web/assets/app.css` MUST define rules for the
following bare HTML elements so a route never falls through to UA
defaults: `h1`, `h2`, `h3`, `h4`, `h5`, `h6`, `p`, `table`, `th`,
`td`, `dl`, `dt`, `dd`, `pre`, `code`, `hr`, `fieldset`, `legend`,
`blockquote`, `figure`, `img`. Every property in those rules MUST
source a custom property declared in `tokens.css`; no literal hex,
`rem`, or `px` value is permitted.

#### Scenario: Bare table renders inside the tokenised contract

- **WHEN** a route emits `<table>` without a class
- **THEN** the rendered page still inherits the tokenised baseline
        (border-collapse, cell padding, border colour from
        `--op-color-border`) and `tests/integration/web_ui_styling.rs`
        fails if the `<table>` lacks `class="table"`, `class="detail__table"`,
        or a BEM-style `*__table` token (defence in depth: a bare
        `<table>` still looks right, but a test pins the affordance).

#### Scenario: Heading visual rhythm sources tokens

- **WHEN** the dashboard renders its 78 `<h1>` and 71 `<h2>` elements
- **THEN** font-size and margin source `--op-font-size-*` and
        `--op-space-*`; the visible cascade matches the design
        token scale and `scripts/scan-template-literals.sh` (in its
        change-3 form) reports zero literal values in the new
        heading block.

#### Scenario: Definition list uses token indent

- **WHEN** `settings.rs` and `software_center.rs` emit a `<dl>` with
        `<dt>` and `<dd>` children
- **THEN** the `<dd>` indent and the `<dt>` gap source
        `--op-space-*`; the UA `40px` indent is gone.

#### Scenario: `pre` no longer overflows at 360 px

- **WHEN** a `<pre>` element contains a long line
- **THEN** the element has `overflow-x: auto` and a tokenised
        background / border; the page body remains within the 360 px
        viewport.

### Requirement: Keyboard Focus Ring Coverage

Every interactive element the user can reach with the keyboard MUST
have a `:focus-visible` rule that paints a 2 px outline in
`var(--op-color-focus-ring)`. The list of interactives is exhaustive:
`a`, `form input`, `form textarea`, `form select`, `form button`,
`.topbar button`, `.table button`, `.btn`, `.button`, `.nav-item`,
`.nav-rail-toggle`, `.op-modal-close`. Adding a new interactive
class without a focus rule is a regression.

#### Scenario: Sidebar item has a focus ring

- **WHEN** the user tabs to a `.nav-item`
- **THEN** a 2 px ring in `var(--op-color-focus-ring)` is visible
        with a 2 px offset.

#### Scenario: Modal close button has a focus ring

- **WHEN** the user tabs to `.op-modal-close`
- **THEN** the ring is visible and uses the same accent colour as
        the form baseline.

#### Scenario: Table-row button has a focus ring

- **WHEN** the user tabs to a `.table button`
- **THEN** the ring is visible; the existing hover style is not
        used as a substitute.

### Requirement: Reduced Motion Respect

`app.css` MUST contain a `@media (prefers-reduced-motion: reduce)`
block that zeroes `animation-duration` and `transition-duration` for
all elements and pauses the `@keyframes op-spin` spinner. Users with
the OS-level reduced-motion preference set see a static spinner and
no transition animation.

#### Scenario: Spinner is static for reduced-motion users

- **WHEN** the user's OS reports `prefers-reduced-motion: reduce`
- **THEN** `.op-loading-spinner` does not animate; CSS transitions
        on `a`, `.card`, `.progress__bar`, and `form button` are
        instant.

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

