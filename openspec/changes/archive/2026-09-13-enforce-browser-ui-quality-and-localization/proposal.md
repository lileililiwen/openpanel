# Proposal: Enforce browser UI quality and localization

## Why

OpenPanel has tokenized CSS, static HTML checks, focus rules, and reusable UI
states, but its declared accessibility workflow is not a live browser gate.
The web UI also contains many literal strings despite the typed translation
contract. Static scans cannot prove rendered contrast, focus visibility,
keyboard behavior, responsive layout, or real localization quality.

## What Changes

- Add a pinned browser test harness with axe and route fixtures.
- Verify WCAG 2.2 AA keyboard, focus, names/roles/states, contrast, target size,
  focus-not-obscured, and reduced-motion behavior.
- Add responsive smoke tests at mobile, tablet, and desktop widths.
- Complete typed translation lookup and detect new literal user-visible strings.
- Add locale fallback, pluralization, timezone/date/number formatting checks.

## Capabilities

### Modified Capabilities

- `web-ui-styling`
- `web-ui`
- `i18n`
- `web-ui-discoverability`

## Non-goals

- No component-framework migration.
- No visual redesign unrelated to accessibility/localization defects.
- No claim of AAA conformance.

## Dependencies

Depends on `unify-capability-navigation-and-site-workspaces` for complete route
inventory and on the quality ratchet for blocking enforcement.
