# Tasks: Web UI element baseline

## 1. Testing

- [x] Add a static contract test in `crates/openpanel-web/src/web_ui_styling.rs`
      that asserts `app.css` contains the new element baseline block
      (`h1 {`, `table {`, `dl {`, `pre {`, `prefers-reduced-motion: reduce`)
      so a future regression fails CI even before the integration walk.
      Implemented as five separate `app_css_*` tests; the
      `app_css_global_element_baseline_is_present` and
      `app_css_element_baseline_blocks_source_tokens` tests together
      assert both presence and token-sourcing.
- [x] Add an integration assertion in `tests/integration/web_ui_styling.rs`
      that walks every public route and fails if a `<table>` renders
      without one of: `class="table"`, `class="detail__table"`, a
      BEM-style `*__table` token, or a subsystem `*-table` token
      (`network-table`, `audit-table`, …). The walk already exists —
      extend the helper, do not duplicate it.
- [x] Add a CSS scan assertion that fails if `app.css` introduces a
      literal hex code or a `rem`/`px` value outside `tokens.css`
      (this is the new contract the change 3 gate will harden — for
      now it lives as a `web_ui_styling.rs` static test).
      Implemented as `app_css_element_baseline_blocks_source_tokens`
      at the unit level (the dedicated scan-literal step is change 3).
- [x] Add a static maud source-level guard in `web_ui_styling.rs`
      (`every_maud_table_has_a_class`) that fails if any
      `<table {` block in `crates/openpanel-web/src/*.rs` lacks a
      `class=` attribute on the same line. Catches the 4 per-resource
      bare tables that the public-route walk cannot reach
      (`/settings/tokens`, `/cron`, `/sites/{id}/ftp`,
      `/sites/{id}/security`).
- [x] Run tests red: `cargo test -p openpanel-web web_ui_styling` and
      `cargo test --test integration web_ui_styling` both fail with
      the missing element baseline and the four bare tables named.

## 2. Implementation

- [x] Add the global element baseline block to
      `crates/openpanel-web/assets/app.css` covering `h1`–`h6`, `p`,
      `table` (no `.table` override), `th`/`td`, `dl`/`dt`/`dd`,
      `pre`, `code` (baseline only — `.config__path code` stays
      untouched), `hr`, `fieldset`, `legend`, `blockquote`, `figure`,
      `img`. All values source `var(--op-*)`.
- [x] Add `:focus-visible` rules for `.topbar button`, `.table button`,
      `.btn`, `.button`, `.nav-rail-toggle`, `.nav-item`, and
      `.op-modal-close` (2 px outline, 1–2 px offset, accent ring).
- [x] Add a `@media (prefers-reduced-motion: reduce)` block that
      zeroes `animation-duration` and `transition-duration` and
      silences `.op-loading-spinner` (the only keyframe in
      `app.css`).
- [x] Convert the 4 bare `<table>` elements to `class="table"`:
      `api_tokens.rs:146`, `ftp.rs:148`, `two_factor.rs:288`,
      `cron.rs:33`.
- [x] Run tests green: the static contract test, the integration
      walk, and the existing `web_ui_styling` suite all pass.

## 3. Verification

- [x] `cargo test -p openpanel-web` — green (185 / 185).
- [x] `cargo test --test integration web_ui_styling` — green
      (10 / 10).
- [x] `make check` — green (fmt, clippy, docs, audit, file-length,
      scan-literal, tasks-testing-first, reuse, layering,
      spec-test-drift, spec-drift, agent-governance,
      governance-contract, test-gates, tests; 15 / 15 gates pass;
      the literal-hex guardrail that change 3 will harden is
      covered at the unit level by
      `app_css_element_baseline_blocks_source_tokens`).
- [x] `openspec validate 2026-09-07-add-web-ui-element-baseline --strict` —
      valid.
- [x] Archive and commit.
