# Proposal: Web UI element baseline

## Why

`app.css` only styles `*`, `html`, `body`, `a`, `ul/ol`, and `code/kbd/pre/samp`.
Every other element falls through to UA defaults:

- 4 `<table>` elements render with no class at all
  (`api_tokens.rs:146`, `ftp.rs:148`, `two_factor.rs:288`, `cron.rs:33`).
- 78 `<h1>` and 71 `<h2>` elements use the UA `2em` / `1.5em` cascade
  with `.67em` / `.83em` margins, breaking the tokenised vertical
  rhythm.
- 92 `<p>` elements use the UA `1em 0` margin and have no
  `max-width` for readability.
- `<dl>/<dt>/<dd>` (`settings.rs:334`, `software_center.rs:1212`)
  use the UA `40px` indent, not a token.
- `<pre>` (4 occurrences) has only `font-family`; no background, no
  padding, no `overflow-x` — long lines break the 360 px viewport.
- `:focus-visible` only covers `a` and `form` descendants. The
  sidebar, topbar, modal close, and table-row buttons are not
  visible when keyboard-focused.

These gaps are felt on every page, not just the few that opt in to
a class. They can only be closed at the element-baseline layer.

## What Changes

- Add a global element baseline to `app.css` for `h1`–`h6`, `p`,
  `table`, `th`/`td`, `dl`/`dt`/`dd`, `pre`, `code`, `hr`, `fieldset`,
  `legend`, `blockquote`, `figure`, `img`, sourcing all values from
  `tokens.css`.
- Replace the 4 bare `<table>` elements in `api_tokens.rs`, `ftp.rs`,
  `two_factor.rs`, `cron.rs` with `class="table"` (the existing
  tokenised `.table` rule already covers them).
- Add `:focus-visible` rules for `.topbar button`, `.table button`,
  `.btn`, `.button`, `.nav-rail-toggle`, `.nav-item`, and
  `.op-modal-close`.
- Add a `@media (prefers-reduced-motion: reduce)` block that
  disables the spinner animation and shortens `transition-duration`
  to `0ms`.
- Add an `overflow-x: auto` rule for `.table-card` and a one-line
  contract assertion in `web_ui_styling.rs` so the baseline can
  never silently regress.

## Capabilities

### Modified Capabilities

- `web-ui-styling`: a new requirement **Global Element Baseline**
  pins the element selectors and the focus-ring coverage; a new
  requirement **Reduced Motion Respect** pins the
  `prefers-reduced-motion` block.

## Impact

Affects: `crates/openpanel-web/assets/app.css`,
`crates/openpanel-web/src/api_tokens.rs`, `ftp.rs`, `two_factor.rs`,
`cron.rs`, `web_ui_styling.rs`, `tests/integration/web_ui_styling.rs`.

No domain, app, or API changes. No new dependencies. The change
ships entirely in the web crate.
