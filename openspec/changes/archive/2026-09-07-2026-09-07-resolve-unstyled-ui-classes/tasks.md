# Tasks: Resolve unstyled UI classes

## 1. Testing

- [ ] Add a static contract test in `web_ui_styling.rs` that scans
      every `class="..."` literal in `crates/openpanel-web/src/`
      and asserts each token appears as a rule selector in
      `app.css`. Dynamic-prefix tokens (`audit-row`,
      `audit-outcome-*`, `audit-badge-*`, `gauge-status-*`,
      `disk-status`, `op-status-fresh`, `op-status-stale`,
      `status-*`) are expanded to their concrete variants in a
      small fixture table; the test fails if a literal token has
      no rule.
- [ ] Add an integration assertion that walks every public route
      and fails if a rendered HTML document contains a class
      attribute that the test's allowed-set does not list (the
      allowed-set is `app.css`'s class-token list, regenerated
      once per CI run).
- [ ] Run tests red: `cargo test -p openpanel-web web_ui_styling`
      and `cargo test --test web_ui_styling` both fail with the
      list of undefined class tokens and their files.

## 2. Implementation

- [ ] Add the "Per-subsystem components" section to
      `crates/openpanel-web/assets/app.css` per the design (audit,
      status page, dashboard, site workspace tabs, settings, form,
      status / dynamic, misc). All values source `var(--op-*)`.
- [ ] De-duplicate `.card`: rename the storefront card to
      `.storefront__card` in `app.css` and at the two callsites in
      `software_center.rs`.
- [ ] Add alias rules so `.button` / `.button--ghost` /
      `.button--danger` / `.empty` / `.empty-state` / `.error` /
      `.error-state` / `.breadcrumb` reuse the canonical
      declarations (no behaviour change, no callsite churn).
- [ ] Add the `form label.checkbox` rule that overrides the
      `flex-direction: column` of the form baseline so the
      checkbox and its text sit inline.
- [ ] Rename the single `class="breadcrumb"` (singular) to
      `class="breadcrumbs"` (plural) so it picks up the existing
      rule.
- [ ] Run tests green.

## 3. Verification

- [ ] `cargo test -p openpanel-web` — green.
- [ ] `cargo test --test web_ui_styling` — green.
- [ ] `make check` — green (no new clippy / fmt / audit / file-length
      regressions; the literal-hex exposure that change 3 will
      clean up is acknowledged but not in scope here).
- [ ] `openspec validate 2026-09-07-resolve-unstyled-ui-classes
      --strict` — valid.
- [ ] Archive after human design approval.
