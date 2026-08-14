# Add Web UI styling and responsive layout — Tasks

## 1. Testing

- [ ] 1.1 Unit: every existing `<form>` in the templates carries a
      class from the global set (`form`, `form form-grid`,
      `form form-row`). Bare `<form>` is a regression and the scan
      fails CI.
- [ ] 1.2 Property: scanning `app.css` finds zero
      `@media (max-width: …)` queries and zero literal hex colours
      outside `tokens.css`.
- [ ] 3 Service: every public web route renders the global form
      rules and the focus-visible ring at all three breakpoints.
- [ ] 1.4 Integration:
      `tests/integration/web_ui_styling.rs` loads each route at
      360 / 768 / 1280 px and asserts no horizontal overflow on
      `<body>`, every `<form>` carries a class, and every `<input>`
      has a paired `<label>`.
- [ ] 1.5 Web: focus indicator, contrast ratio, and label
      accessibility are confirmed for every form.

## 2. Domain and Application

- [ ] 2.1 No domain changes (this is a UI-only change).
- [ ] 2.2 Rewrite `crates/openpanel-web/assets/app.css` so the
      global `form` / `form-grid` / `form-row` rules replace the
      legacy `.form` / `.login` opt-ins.
- [ ] 2.3 Migrate every existing `<form>` in
      `crates/openpanel-web/src/` to the global classes (in the
      order listed in `design.md`).

## 3. Adapters and UI

- [ ] 3.1 Add the responsive breakpoints (mobile / tablet /
      desktop) to `app.css` using the mobile-first `min-width`
      pattern.
- [ ] 3.2 Add the `table-card` scroll container for wide tables.
- [ ] 3.3 Wire `tokens.css` (already shipped) into every form's
      colour / spacing / typography.

## 4. Validation

- [ ] 4.1 `cargo test --workspace` twice.
- [ ] 4.2 `make check` clean (no new clippy warnings; pre-existing
      quality failure tolerated).
- [ ] 4.3 Smoke-test at 360 / 768 / 1280 px in a headless browser;
      capture screenshots and attach to the PR description.
- [ ] 4.4 Archive with
      `openspec archive add-web-ui-styling-and-responsive-layout`.
      The delta is folded into
      `openspec/specs/web-ui-styling/spec.md`.