# Design: Web UI element baseline

## Approach

Add a single new block to `app.css` between the current `code, kbd,
pre, samp` rule and the `.sidebar .nav-item` block. The new block
covers the 14 element selectors listed in the proposal and any
`:focus-visible` gap. All values source `var(--op-*)` from
`tokens.css`; no literal hex / rem / px is introduced.

The four bare `<table>` elements are converted to `class="table"`.
`.table` is already defined (`app.css:535`) and uses tokens; the
touch is one `class="…"` per call site.

The focus-ring rules extend the existing form-baseline pattern
(`form input:focus-visible, form button:focus-visible, ...`) to the
non-form interactives that are currently missing it. They all
source `--op-color-focus-ring`.

The `prefers-reduced-motion` block sets `animation-duration: 0ms`
and `transition-duration: 0ms`; the existing `@keyframes op-spin`
spinner and the `transition` rules on `a`, `card`, `progress__bar`,
`form button` are silenced for users who request it.

## Explore & Reuse

- Reuse `.table` (`app.css:535`) for the four bare tables — no
  new CSS, no token additions.
- Reuse `--op-color-focus-ring` (`tokens.css:37`) and the
  `outline: 2px solid var(--op-color-focus-ring)` pattern
  (`app.css:174`, `app.css:689`).
- Reuse `--op-space-*` (`tokens.css:40-46`) and `--op-radius-md`
  (`tokens.css:50`) for all new spacing and radii.
- Reuse `--op-font-size-*` and `--op-line-height` for the heading
  scale and the `p` `max-width` derived from `--op-line-height`.

No new tokens are added. The existing vocabulary is sufficient.

## Non-goals

- No new design language, no redesign of the heading scale beyond
  sane tokenised defaults.
- No light-theme work — `html[data-theme="light"]` is untouched in
  this change; the literal-hex exposure in `.detail` / `.table
  .status` is the focus of change 3.
- No class-resolution gate — that lives in change 3.
- No unstyled class work (`.site-tabs`, `.audit-*`, `.status-*`,
  etc.) — that lives in change 2.

## Files Touched

- `crates/openpanel-web/assets/app.css` — new element-baseline
  block, focus-ring rules, `prefers-reduced-motion` block.
- `crates/openpanel-web/src/api_tokens.rs:146` —
  `table {` → `table class="table" {`.
- `crates/openpanel-web/src/ftp.rs:148` — same.
- `crates/openpanel-web/src/two_factor.rs:288` — same.
- `crates/openpanel-web/src/cron.rs:33` — same.
- `crates/openpanel-web/src/web_ui_styling.rs` — add a static
  contract that fails if the element baseline regresses (e.g.
  asserts `app.css` contains `h1 {` and `prefers-reduced-motion`).
- `tests/integration/web_ui_styling.rs` — extend the route walk to
  assert no `<table>` element renders without `class="table"`,
  `class="detail__table"`, or a BEM-style `*__table` token.
- `openspec/specs/web-ui-styling/spec.md` — two new requirements.
