# Spec delta: web-ui-styling

## ADDED Requirements

### Requirement: Stylesheet Is Self-Scanned

The `scan-literal` gate MUST cover `crates/openpanel-web/assets/*.css`
in addition to the existing `.rs` / `.maud` / `.html` scope. The
gate's `*/assets/*` exclusion is removed. The gate's allowlist
continues to recognise `tokens.css` as the colour home; every
literal hex, `rgb()`, `hsl()`, and `rgba()` in any other file —
including `app.css` — fails the build.

#### Scenario: Literal hex in app.css fails the scan

- **WHEN** a PR adds `#ff00ff` inside `app.css`
- **THEN** `make scan-literal` fails with the offending file and
        line; the `step: scan-literal status: failed` line is
        printed.

#### Scenario: Literal rgba in app.css fails the scan

- **WHEN** a PR adds `rgba(255, 0, 255, 0.5)` inside `app.css`
- **THEN** the same failure fires; the existing
        `rgba(var(--op-color-accent-rgb), 0.5)` pattern is the
        only way to express a translucent accent.

#### Scenario: tokens.css remains in the allowlist

- **WHEN** `tokens.css` declares `--op-color-accent: #4f8cff;`
- **THEN** the scan ignores it (it is the colour home); the rule
        is the source of truth for the value.

### Requirement: Class Coverage Is CI-Enforced

`make check` MUST include a `class-coverage` step that fails the
build if any `class="..."` literal in
`crates/openpanel-web/src/*.rs` references a token that has no
matching rule selector in `app.css`. The gate's contract:

- It greps every literal `class="..."` attribute.
- It expands a fixture table of dynamic-prefix families
  (`audit-row`, `audit-outcome-*`, `audit-badge-*`,
  `gauge-status-*`, `disk-status-*`, `op-status-fresh`,
  `op-status-stale`, `status-*`) to their concrete variants
  before checking.
- It ignores a small `IGNORED_TOKENS` list (e.g. `inline`,
  which is a reserved HTML attribute name and is already covered
  by `form[class~="inline"]`).
- It prints `step: class-coverage status: failed` and the
  offending file:token pair on failure.

#### Scenario: A new undefined class fails the build

- **WHEN** a PR adds `class="brand-new-widget"` to a route
- **THEN** `make class-coverage` fails with the file path and
        the token; the PR is blocked from merging until the
        matching rule is added.

#### Scenario: A new dynamic variant fails the build

- **WHEN** a PR extends the `audit-outcome-` family to
        `audit-outcome-pending` without adding a rule
- **THEN** the gate fails with the missing variant; the
        fixture table in `scripts/check-class-coverage.sh` is
        updated in the same PR.

#### Scenario: A legitimately removed class passes

- **WHEN** a PR removes a class from every callsite but
        accidentally leaves the rule in `app.css`
- **THEN** the gate passes (the gate only checks
        used-but-undefined, not defined-but-unused); dead rules
        are caught by the existing file-length / clippy gates
        or by a future dead-css lint.

## MODIFIED Requirements

### Requirement: Token-Only Styling

Every CSS rule outside `tokens.css` MUST source colours, spacing,
radii, and typography from the token custom properties declared in
`tokens.css`. Literal hex codes, pixel-spacing values, or non-token
font families are forbidden outside `tokens.css`. Enforcement is
provided by the `scan-literal` gate (which now also covers
`app.css`) and the `class-coverage` gate (which verifies every
`class="..."` literal has a matching rule).

#### Scenario: CSS uses tokens

- **WHEN** the quality scan runs over `app.css`
- **THEN** the scan reports zero literal hex colours, zero
        `@media (max-width: …)` queries, and zero non-token pixel
        spacing values; CI fails otherwise.

#### Scenario: Class coverage holds the source in lockstep

- **WHEN** a PR adds a new `class="x"` attribute to a route
        without adding a `.x { ... }` rule in `app.css`
- **THEN** `make class-coverage` fails with the file:token pair.
        The Token-Only Styling contract therefore covers both
        property values (no literals) and selector coverage
        (every class is defined).
