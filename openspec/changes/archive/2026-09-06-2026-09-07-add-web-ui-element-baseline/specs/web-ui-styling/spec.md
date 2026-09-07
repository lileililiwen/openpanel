# Spec delta: web-ui-styling

## ADDED Requirements

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
