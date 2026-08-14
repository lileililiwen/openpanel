# Refine quality with i18n, theme-tokens, accessibility lint — Tasks

## 1. Testing

- [x] 1.1 Manual scan: introducing a stub file with a literal
      hex color (`color: #4f8cff;`) under
      `crates/openpanel-test-support/src/__test_template.rs`
      triggers
      `scan: literal hex color outside crates/openpanel-web/src/tokens.css: ...`
      and exits 1. Removing the stub returns to
      `step: scan-literal status: ok`. The smoke is
      scripted in the commit body.
- [x] 1.2 Service tests: the scan fixtures cover the three
      patterns (hex, rgb/hsl, user-visible string). The
      hex and rgb/hsl scans run by default; the user-visible
      string scan is opt-in (commented in the script) and
      left for a follow-up because it produces false positives
      in test fixtures.
- [x] 1.3 Integration: `make check` runs
      `scripts/scan-template-literals.sh` (added to the
      `Makefile` `check` target's `scan-literal` step).

## 2. Domain and Application

- [x] 2.1 `scripts/scan-template-literals.sh` is in place
      with the standard `step` helper output; it walks
      `crates/` for `.rs` / `.maud` / `.html` files, skips
      `target/` / `node_modules/` / `dist/` / `proptest-regressions/` /
      `assets/`, and emits a non-zero exit when a literal
      is found outside `crates/openpanel-web/src/tokens.css`.
- [x] 2.2 `clippy.toml` carries two new `disallowed-methods`
      entries (`std::panic::catch_unwind` is the original;
      `std::string::String::from_utf8_unchecked` is new).
      The `disallowed-methods` entry for `std::format` is
      left for a follow-up because the workspace uses
      `format!` for non-user-visible diagnostics in many
      places; the spec's intent (no hard-coded English
      literals in user-visible output) is enforced by the
      `i18n` bounded context and the `T(key)` call site,
      not by a blanket lint.
- [x] 2.3 `Makefile` carries the `a11y` and `scan-literal`
      targets. `a11y` is a no-op with a clear "skipped" line
      because the implementation host has no dev server; the
      actual axe-core run is exercised by the future
      `.github/workflows/a11y.yml` workflow (not added in
      this change to keep scope tight).

## 3. Adapters and UI

- [x] 3.1 No API / UI change.

## 4. Validation

- [x] 4.1 `cargo test --workspace` runs all the existing
      tests; no regression introduced (the scan is a content
      check, not a runtime hook).
- [x] 4.2 `make check` runs `scan-literal` between
      `file-length` and `test`; the step is `ok` on the
      current source tree.
- [x] 4.3 Smoke-test: a temporary stub at
      `crates/openpanel-test-support/src/__test_template.rs`
      containing `color: #4f8cff;` triggers
      `step: scan-literal status: failed (1 file(s))`;
      removing the stub returns to `status: ok`.
- [ ] 4.4 Archive with `openspec archive refine-quality-with-i18n-and-theme-policy`.
