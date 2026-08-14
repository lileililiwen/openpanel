# Refine quality with i18n, theme-tokens, and accessibility lint rules

## Why

`openspec/specs/quality/spec.md` defines clippy, fmt, audit, and
doc gates. The follow-on refinements (web-ui with audit /
accessibility / theming) and new changes (i18n, accessibility-
baseline, themeable-ui) need lint rules that catch policy
violations: literal colours outside `tokens.css`, literal
user-visible strings outside the string table, and accessibility
smoke failures. This refinement codifies these as `cargo` lint
and content-scan checks.

## What Changes

- New content-scan script `scripts/scan-template-literals.sh`
  that fails the build when literal colours (hex / rgb / hsl) or
  user-visible strings appear outside their approved homes
  (`tokens.css` and `t.rs`).
- New `clippy.toml` entries disallowing `format!("…")` of
  hard-coded English that escapes the locale negotiation path.
- New `make a11y` target invoking axe-core against the running
  dev server (skipped if no dev server is available).

## Capabilities

### Modified Capabilities

- `quality`: content-scan script; clippy tightenings; a11y gate.

## Impact

- `clippy.toml`: append three `disallowed-methods` entries.
- `scripts/scan-template-literals.sh`: new file.
- `Makefile`: append `a11y` target.
