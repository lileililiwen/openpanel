# Design: Enforce browser UI quality and localization

## Approach

Use a small pinned Playwright/axe harness against the existing server-rendered
HTMX application. Route metadata supplies the route matrix; fixtures create
the minimum authenticated roles and data. Translation keys remain resolved at
render time through the existing `t` module, with a source gate rejecting
literal user-visible strings in templates and attributes.

## Explore & Reuse

- Reuse `tests/integration/web_ui_styling.rs`, `web_ui_audit.rs`, `TestServer`,
  `CapabilitySet`, and existing CSS token contracts.
- Reuse `crates/openpanel-web/src/t.rs`, `Shell::render`, and existing locale
  preference persistence.
- Reuse `ui_states`, semantic tables, focus-visible CSS, and reduced-motion
  tokens rather than adding duplicate components.

## Boundaries

Browser tests prove rendered behavior; source tests enforce translation and
token contracts. The web crate remains server-rendered and framework-free at
runtime. Accessibility failures block CI; environment startup failures are
reported as infrastructure failures.

## Verification

Run route-wide browser tests for each role and viewport, source scans, locale
fallback tests, and `make check`. Capture artifacts for browser failures.

## Non-goals

- Replacing HTMX or Maud.
- Automated remediation of arbitrary copy.
- Full manual usability research.
