# Proposal: Tokenise app.css and add the class-coverage gate

## Why

Two contracts in `tokens.css:5` and the existing
`web-ui-styling` requirement *Token-Only Styling* say
"every CSS rule outside `tokens.css` MUST source colours,
spacing, radii, and typography from token custom properties; no
literal hex, pixel, or non-token value is permitted." Two leaks
undermine the contract today:

1. **`app.css` itself contains literal values.** The
   `.storefront` / `.detail__pane` / `.table .status` / `.table
   button` / `.banner--ok` / `.button` / `.progress__bar` blocks
   use raw `rem` / `px` (e.g. `0.875rem`, `0.5rem`) and raw hex
   (`#3fb950`, `#f85149`, `#d29922`, `#4493f8`, `white`). The
   consequence: `html[data-theme="light"]` only remaps the
   six legacy aliases (`--bg`, `--panel`, `--border`, `--text`,
   `--muted`, `--accent`). Status pills, banners, the progress
   bar, and the storefront button stay dark on a light theme.
   Light theme is broken on every page that uses a status pill
   or a banner.

2. **`scripts/scan-template-literals.sh:32` excludes `*/assets/*`.**
   The same literal-hex ban that catches a `#3fb950` in a maud
   template is silent inside the stylesheet that produces the
   final pixel. The two changes proposed here close that hole.

A second gap is the missing class-coverage gate. Changes 1 and 2
ship the content (the element baseline, the 87 missing class
rules), but without a gate the same regression can return on the
next PR. The gate belongs in this change so the contract is
self-enforcing.

## What Changes

- Replace every literal hex / `rem` / `px` value inside
  `crates/openpanel-web/assets/app.css` with a token from
  `tokens.css`. Add new tokens to `tokens.css` only where the
  current vocabulary is insufficient (e.g. a `--op-color-success-rgb`
  triplet for the existing `rgba(63, 185, 80, 0.08)` patterns, a
  `--op-font-size-2xs` if `0.75rem` is recurring).
- Remove the `*/assets/*` exclusion from
  `scripts/scan-template-literals.sh` so the existing literal
  scan covers `app.css` as well. Add a paired negative-fixture in
  `scripts/test-gates.sh` so the gate is exercised both ways.
- Add a new `scripts/check-class-coverage.sh` (and a `make
  class-coverage` target wired into `make check`) that:
  1. Greps every `class="..."` literal in `crates/openpanel-web/src/`.
  2. Expands the dynamic-class-family fixture table.
  3. For every concrete class token, asserts the corresponding
     rule exists in `app.css`.
  4. Fails the build with the offending file:token pair.
- Add positive + negative fixtures for `class-coverage` in
  `scripts/test-gates.sh`.

## Capabilities

### Modified Capabilities

- `web-ui-styling`: a new requirement **Stylesheet Is Self-Scanned**
  pins the `scan-literal` extension to `assets/`. A second new
  requirement **Class Coverage Is CI-Enforced** pins the new
  `check-class-coverage.sh` gate.
- `quality`: extend the existing "Scan-Literal Gate" requirement
  to name the new exit conditions, so `make check` fails on
  literal values in `app.css`.

## Impact

Affects: `crates/openpanel-web/assets/tokens.css`,
`crates/openpanel-web/assets/app.css`,
`scripts/scan-template-literals.sh`, `scripts/test-gates.sh`,
`scripts/check-class-coverage.sh` (new), `Makefile`,
`tests/integration/web_ui_styling.rs` (smoke test for the new
script).

No domain / app / API changes. No new dependencies.
