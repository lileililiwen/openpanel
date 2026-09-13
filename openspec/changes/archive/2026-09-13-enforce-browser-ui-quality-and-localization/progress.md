# Progress: enforce-browser-ui-quality-and-localization

## Design approval (2026-09-13, standing principal direction)

HANDOFF.md "Next steps" pre-grants design approval for the remaining
maturity changes (automatic approve). Dependency on changes 1
(`ratchet-quality-and-spec-maturity`) and 6
(`unify-capability-navigation-and-site-workspaces`, commit `c31aa19`)
is satisfied: the route/capability registry this change consumes is
live in `crates/openpanel-web/src/capability_registry.rs`.

Design (`design.md`): pinned Playwright/axe harness against the
server-rendered HTMX app, route matrix from the registry, fixtures
for minimum authenticated roles/data, typed `t` resolution with a
source gate, reuse of `web_ui_styling` contracts / `t` / `Shell` /
`ui_states` / focus-visible + reduced-motion tokens. No framework
migration, no AAA claim. APPROVED for implementation.

## Research (2026-09-13)

- Domain `openpanel-domain/src/i18n` already ships `Locale`,
  `Catalog`, `CatalogResolver` (exact → family → default fallback),
  `LocaleNegotiator` (URL → user → Accept-Language → default),
  `Formatter` (number/currency/date/time, RTL `dir`), `plural_category`
  (en/de/pl/ar/zh + default), `render_template` (`{name}`).
- App `openpanel-app/src/i18n.rs` ships `default_catalog` (6 keys),
  `LocaleService` (negotiate/resolve/render/plural + user prefs),
  `TranslationEntry`. Web `t.rs` is still a stub (`t(key)` returns the
  key; override cell for tests). Templates use literal strings.
- `capability_registry::REGISTRY` (41 entries: 27 global + 14 site) is
  the single inventory; `CapabilitySet::shipped()` derives from it.
- Existing gates already pin: mobile-first `@media (min-width: …)`,
  `:focus-visible` ring, `prefers-reduced-motion` block, class-coverage,
  `scan-literal` (colours only), `web_ui_styling` integration (forms /
  labels / tables / inline-width at 360/768/1280). `make a11y`
  currently skips; no axe/browser gate is wired into `make check`.
- `Shell::render` emits `html lang=` but no `dir` (RTL gap for `ar-SA`).

## Plan (2026-09-13)

1. New `openpanel-web/src/browser_ui_quality.rs` (pure, I/O-free):
   route matrix derived from the registry, static accessibility /
   responsive evaluators over rendered HTML, typed catalog resolution
   with fallback reporting (reusing domain `CatalogResolver` /
   `Formatter` / `render_template` + app `default_catalog`), locale
   formatting helpers delegating to `Formatter`, CSS contract probes
   (focus ring, reduced motion), artifact writer. Unique `browser_*`
   names so `reuse-strict` stays green.
2. Shell `dir` attribute via `Formatter::dir` (incremental subsystem
   migration: shell metadata; full template migration stays a
   follow-up — same deferral precedent as file/backup workflows).
3. `scripts/check-browser-ui-quality.sh` + `make browser-ui-quality`
   wired into `make check`; skipped cleanly when browsers are absent
   (infrastructure failure per design, not a blocking failure).
4. Pinned `tests/browser/package.json` (playwright + axe-core) +
   `tests/browser/quality.mjs` route-driven harness with artifact
   capture + `.github/workflows/browser-ui-quality.yml` (install/cache,
   blocking axe run, artifact upload).
5. Integration `tests/integration/browser_ui_quality.rs` (role × route
   × viewport static gates + localization fallback/plural/date/number)
   referencing `browser-ui-quality` for `spec-test-drift-strict`.
6. `make check` + `openspec validate --strict`, tick `tasks.md` with
   evidence, archive, two-commit cadence.

## Implementation (2026-09-13)

- `crates/openpanel-web/src/browser_ui_quality.rs` (new, ~600 lines):
  registry-derived route matrix (`browser_matrix_paths`,
  `browser_paths_for_role`, `browser_concrete_path`), static
  accessibility / responsive evaluators, catalog resolution with
  fallback reporting (`browser_resolve_text`, `browser_render_text`),
  formatting helpers delegating to domain `Formatter`
  (`browser_format_number/currency/date/time`, `browser_text_dir`),
  CSS probes (`browser_focus_ring_is_present`,
  `browser_reduced_motion_is_present`), artifact writer
  (`browser_artifact_filename`, `browser_write_artifact`). 14 unit
  tests green.
- `layout::Shell::render` now emits `dir=` via `browser_text_dir`
  (shell-metadata subsystem migration; `ar-SA` → `rtl`).
- `scripts/check-browser-ui-quality.sh` + `make browser-ui-quality`
  wired into `make check` after `class-coverage`; live browser leg
  skips cleanly without `OPENPANEL_BROWSER_BASE_URL`.
- `tests/browser/package.json` (playwright 1.49.1, axe-core 4.10.2) +
  `quality.mjs` (route × viewport axe + overflow + reduced-motion,
  artifact capture) + `.github/workflows/browser-ui-quality.yml`.
- `tests/integration/browser_ui_quality.rs` (7 tests: unauth matrix,
  Owner/Admin/User accessible, 3-viewport responsive, CSS probes,
  localization fallback/plural/format + shell `dir`).
- `Cargo.toml` dev-deps gain `openpanel-web` (integration reuse, no
  logic duplication); `AGENTS.md` / `Agents.md` gate lists updated.
- Full-template literal migration deferred to the follow-up (pre-existing
  literals; day-one hard gate would be red on untouched files).

## Verification (2026-09-13)

- `cargo test -p openpanel-web --lib browser_ui_quality`: 14/14 green.
- `cargo test --test integration browser_ui_quality`: 7/7 green.
- `scripts/check-browser-ui-quality.sh`: ok (live leg skipped locally).
- `openspec validate enforce-browser-ui-quality-and-localization --strict`: valid.
- `make check`: green end-to-end (fmt, clippy, docs, audit, file-length,
  scan-literal, class-coverage, browser-ui-quality, release-governance,
  tasks-testing-first, reuse-strict, layering, spec-test-drift-strict
  [only tracked baseline debt], spec-drift, agent-governance,
  governance-contract, coverage-floor skipped/tool-absent-not-required,
  maturity, test-gates 71/71, test incl. 217 openpanel-web lib tests).
- Gate is blocking: unit/CSS/harness/axe failures exit non-zero;
  artifacts upload on workflow failure.
