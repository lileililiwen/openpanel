# Design: Tokenise app.css and add the class-coverage gate

## Approach

### 1. Audit the literals in `app.css`

The literal scan that the existing `scripts/scan-template-literals.sh`
performs on Rust / maud / HTML will, after the exclusion is removed,
report every hex and every `rgb()` / `hsl()` in `app.css`. The
expected shortlist (verified manually) is roughly:

- Hex: `#3fb950` (success), `#d29922` (warning), `#f85149` (danger),
  `#4493f8` (progress), `white`, `#ff6b6b` (table-button danger
  hover), `#f85149` (modal text).
- `rem`/`px` literals in the storefront / `.detail` / `.table
  button` / `.progress` blocks: `0.75rem`, `0.875rem`, `1rem`,
  `1.5rem`, `2rem`, `0.125rem`, `0.25rem`, `0.375rem`, `0.5rem`,
  `0.625rem`, `8px`.
- `rgba(248, 81, 73, 0.08)`, `rgba(63, 185, 80, 0.08)` /
  `rgba(63, 185, 80, 0.12)` / `rgba(63, 185, 80, 0.2)`,
  `rgba(210, 153, 34, 0.2)`, `rgba(79, 140, 255, 0.1)` /
  `rgba(79, 140, 255, 0.15)` / `rgba(79, 140, 255, 0.16)`.

### 2. New tokens

Add to `tokens.css` only what is missing and recurring. The
proposed additions:

- `--op-color-success-rgb: 63, 185, 80;` and `--op-color-warning-rgb:
  210, 153, 34;` and `--op-color-danger-rgb: 248, 81, 73;` and
  `--op-color-accent-rgb: 79, 140, 255;`. These let
  `rgba(var(--op-color-success-rgb), 0.12)` replace the literal
  `rgba(63, 185, 80, 0.12)`. The hex tokens stay for the cases
  that need a solid colour.
- `--op-font-size-2xs: 11px;` so `0.75rem` (12px in 16px root,
  but `0.75rem` is used as a semantic font-size in the storefront
  for "tiny") becomes a token. (A small step: keeping the
  root-relative unit means we do not need to change every value
  to `px`.)
- `--op-space-half: 2px;` so `0.125rem` and `2px` map to one
  token.

### 3. Light-theme remap

Extend `html[data-theme="light"]` to remap the same `--op-color-*`
tokens where the dark hex reads as low contrast on light surfaces.
Concretely:

- `--op-color-success: #1a7f37;` (darker green for a light
  background)
- `--op-color-warning: #9a6700;`
- `--op-color-danger: #b1101b;`
- `--op-color-info: #0969da;`
- `--op-color-accent-fg: #ffffff;` (unchanged — the button stays
  white-on-accent)

The hex on the storefront's `.storefront__tabs .tab--active`
(`background: var(--accent)` and `color: white`) is fine because
`--accent` itself is remapped in light theme (`#2869dc`).

### 4. The `scan-literal` extension

Edit `scripts/scan-template-literals.sh` to drop the
`*/assets/*` case from the path filter. The script already
excludes `*/target/*`, `*/node_modules/*`, `*/dist/*`,
`*/proptest-regressions/*`. The current line is:

```sh
case "$f" in
    */target/*|*/node_modules/*|*/dist/*|*/proptest-regressions/*|*/assets/*) continue ;;
esac
```

The fix is one line: drop the `*/assets/*` entry. The script
already handles `.css` (it iterates over `*.rs`, `*.maud`,
`*.html` only — see line 47). We extend the find pattern to
include `*.css` and the scanner will pick up `app.css` and
`tokens.css`. The `tokens.css` is in the allowlist (`ALLOWED_COLOUR_HOME`),
so it will continue to pass; the new `app.css` rules will fail
until this change replaces them.

### 5. The `check-class-coverage.sh` gate

A new `scripts/check-class-coverage.sh` (modeled on the existing
`check-tasks-testing-first.sh` and the other one-script-per-gate
scripts):

1. Run `rg --no-filename -hoE 'class="[^"]*"' crates/openpanel-web/src/`
   to get every literal class attribute. Pipe through `tr ' ' '\n'` to
   get a per-token stream.
2. For each token, assert a rule exists in `app.css`. A rule match
   is one of:
   - `^\.{token}\b` (the token itself is a selector).
   - `^\.{family}-{variant}\b` for the dynamic-prefix family
     expansions in a fixture table near the top of the script.
3. Allow an `IGNORED_TOKENS` list (e.g. `inline` — a legacy
   reserved word that appears in 60 places and is already covered
   by `form[class~="inline"]`).
4. Print `step: class-coverage status: ok | failed` and exit
   non-zero on any missing rule.

Wired into `Makefile` with a new `.PHONY` entry and added to
the `check:` target chain between `scan-literal` and
`tasks-testing-first`.

### 6. Test-gates fixtures

Add to `scripts/test-gates.sh` two new lines, modeled on the
existing `# checker: <id> positive` / `negative` pattern:

- `# checker: class-coverage positive` — `bash scripts/check-class-coverage.sh`
  exits 0 against the current source.
- `# checker: class-coverage negative` — a temporary fixture in
  `tests/fixtures/` adds a `class="made-up-token"` and the same
  script exits non-zero. The fixture is removed before commit;
  the test-gates run uses a copy in a tmp dir.

## Explore & Reuse

- Reuse the existing `check-*` script shape (every other gate is
  one shell script, exits 0/non-zero, prints
  `step: <name> status: ok | failed`).
- Reuse the existing `scripts/lib/step.sh` source line and
  pattern.
- Reuse the existing test-gates fixture convention.
- Reuse the existing literal scan (`scan-template-literals.sh`)
  rather than inventing a parallel colour scan.
- Reuse the existing token vocabulary in `tokens.css`; only add
  `-rgb` triplets and the two small sizing tokens listed above.

No new build tools, no new dependencies.

## Non-goals

- No new design language, no new component vocabulary. This
  change only tokenises and gates.
- No regression of the existing 127 working class rules.
- No change to the storefront's BEM-style class names
  (`storefront__header`, etc.) — they are already token-friendly.

## Files Touched

- `crates/openpanel-web/assets/tokens.css` — add the
  `--op-color-*-rgb` triplets and the `--op-space-half`,
  `--op-font-size-2xs` tokens.
- `crates/openpanel-web/assets/app.css` — replace every literal
  hex / rem / px with the corresponding token.
- `scripts/scan-template-literals.sh` — drop the `*/assets/*`
  exclusion; add `*.css` to the find pattern.
- `scripts/check-class-coverage.sh` — new script.
- `scripts/test-gates.sh` — new positive + negative fixtures.
- `Makefile` — new `class-coverage` target, wired into `check`.
- `tests/integration/web_ui_styling.rs` — small smoke test that
  asserts `scripts/check-class-coverage.sh` is wired into
  `make check` (parse the Makefile target list and check).
- `openspec/specs/web-ui-styling/spec.md` — two new requirements.
- `openspec/specs/quality/spec.md` — extend the existing
  scan-literal requirement.
