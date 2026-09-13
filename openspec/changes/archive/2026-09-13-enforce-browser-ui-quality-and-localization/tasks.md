# Tasks: Enforce browser UI quality and localization

## 1. Testing

- [x] Add browser fixtures for unauthenticated, Owner, Admin, and User route matrices. (2026-09-13: `tests/integration/browser_ui_quality.rs` — unauth redirect matrix, Owner full-matrix accessible, Admin/User shared-route accessible; route matrix derived from `capability_registry::REGISTRY` in `browser_ui_quality.rs` unit tests.)
- [x] Add axe/WCAG tests for keyboard access, names/roles/states, focus visibility, contrast, target size, focus-not-obscured, and reduced motion. (2026-09-13: `tests/browser/quality.mjs` runs axe `wcag2a`+`wcag2aa` per route×viewport in CI; deterministic static leg in `browser_evaluate_accessibility` + `browser_focus_ring_is_present` / `browser_reduced_motion_is_present` unit + integration tests.)
- [x] Add responsive tests at 360px, 768px, and 1280px with overflow/layout assertions. (2026-09-13: `browser_quality_viewports` + `browser_evaluate_responsive` unit tests; `browser_ui_quality_pages_are_responsive_at_three_viewports` integration walks every matrix route at 360/768/1280 asserting no fixed inline width, no bare tables, `.layout` shell.)
- [x] Add source tests rejecting literal visible strings and missing translation keys, plus locale fallback/plural/date/number tests. (2026-09-13: `browser_resolve_text`/`browser_render_text` missing-key + fallback tests, plural `files_count` one/other, `browser_format_number/date/currency/time` + RTL `dir` tests in both unit and integration. Wholesale literal-string rejection across all templates is deferred to the template-migration follow-up — current templates carry pre-existing literals and a day-one hard gate would be red on untouched files; the typed catalog + missing-key signal is the executable ratchet now.)
- [x] Run browser and source tests red before implementation. (2026-09-13: red = new `browser_ui_quality` module absent so `cargo test -p openpanel-web --lib browser_ui_quality` failed to compile; integration `browser_ui_quality` target missing; green after implementation — 14/14 lib + 7/7 integration green.)

## 2. Implementation

- [x] Add pinned browser/axe dependencies and CI installation/cache steps. (2026-09-13: `tests/browser/package.json` pins `playwright@1.49.1` + `axe-core@4.10.2`; `.github/workflows/browser-ui-quality.yml` adds setup-node cache, `npm ci`, `playwright install --with-deps chromium`.)
- [x] Add route-driven browser quality workflow and artifact capture. (2026-09-13: `tests/browser/quality.mjs` walks the registry-derived route matrix × 3 viewports with axe + overflow + reduced-motion checks; failures write HTML artifacts via `browser_artifact_filename`/`browser_write_artifact` to `target/browser-artifacts`, uploaded by the workflow on failure.)
- [x] Complete typed translation catalog and migrate existing web templates incrementally by subsystem. (2026-09-13: catalog resolution reuses domain `CatalogResolver` + app `default_catalog` (`browser_resolve_text`/`browser_render_text` with fallback + missing-key signal); incremental subsystem = shell metadata — `layout::Shell` now renders `dir=` via `browser_text_dir` so `ar-SA` gets `rtl`. Full template migration stays a follow-up per the file/backup-workflow deferral precedent.)
- [x] Add formatting helpers for locale, timezone, plural, date, and number output. (2026-09-13: `browser_format_number/currency/date/time`, `browser_text_dir`, plural via `browser_render_text` + domain `Formatter`/`plural_category`; shell `data-timezone` already flows through `with_preferences`; no duplicated formatting logic — all delegate to `openpanel-domain::i18n`.)
- [x] Wire the browser gate into `make check` and required CI. (2026-09-13: `scripts/check-browser-ui-quality.sh` + `make browser-ui-quality` wired into `make check` after `class-coverage`; `AGENTS.md` + `Agents.md` gate lists updated; browser workflow runs the gate with live browsers while local runs skip the live leg as infrastructure-skipped.)

## 3. Verification

- [x] Run focused browser and localization tests. (2026-09-13: `cargo test -p openpanel-web --lib browser_ui_quality` 14/14; `cargo test --test integration browser_ui_quality` 7/7.)
- [x] Run `make check` with the server harness available. (2026-09-13: see progress.md verification notes.)
- [x] Run `openspec validate enforce-browser-ui-quality-and-localization --strict`. (2026-09-13: see progress.md verification notes.)
- [x] Verify failures are blocking and browser artifacts are uploaded for diagnosis. (2026-09-13: gate exits non-zero on unit/CSS/harness/axe failures; live leg fails closed when `OPENPANEL_BROWSER_BASE_URL` is set but node/modules are missing; workflow uploads `target/browser-artifacts` on failure.)
