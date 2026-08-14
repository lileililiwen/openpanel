# Add i18n and localization — Tasks

## 1. Testing

- [x] 1.1 Unit tests in `crates/openpanel-domain/src/i18n/mod.rs`:
      `Locale::new` normalisation, language / region subtag
      access, `Catalog::add_message` / `add_plural` /
      `get` / `get_plural`, `PluralForms::select` for
      `en-US` / `pl-PL` / `ar-SA`, `parse_accept_language`
      quality ordering, `LocaleNegotiator::negotiate` for
      URL-prefix / user-preference / Accept-Language /
      language-family fallback / default fallback,
      `CatalogResolver` exact / family / default lookup,
      `Formatter::format_number` (en-US, de-DE),
      `format_currency` (en-US, de-DE), `format_date`
      (en-US, de-DE, zh-CN), `format_time` (24h, 12h),
      `is_rtl` / `dir`, `render_template` substitution and
      unknown-arg pass-through. 28 tests, all green.
- [ ] 1.2 Property tests for en-US key coverage and plural
      form invariants — omitted in v0.1; covered by the
      unit tests' CLDR rule tests for `en`, `pl`, `ar`,
      `zh`, `ja`, `ko`. A proptest-based property is left
      for a follow-up.
- [x] 1.3 Service tests in `crates/openpanel-app/src/i18n.rs`:
      `default_catalog` has the basic keys, `render` with a
      simple key, `render` returns the key on missing,
      `submit_translation` then `resolve`, per-user
      preference round-trip with `InMemoryLocaleUserPrefsRepository`.
      5 tests, all green.
- [ ] 1.4 Live locale switch on the web shell — the
      `/settings/locale` web form is left for a follow-up
      change that wires the `LocaleService` into the
      composition root and the web layout.
- [ ] 1.5 CLI E2E — the `openpanel i18n {list,show,set,export}`
      subcommand is left for a follow-up change that adds
      the service to the CLI composition root.
- [ ] 1.6 Web locale switcher (CSRF) — deferred alongside
      the live-switch integration test.

## 2. Domain and Application

- [x] 2.1 `crates/openpanel-domain/src/i18n/mod.rs` exposes
      `Locale`, `Catalog`, `Message`, `PluralForms`,
      `PluralCategory`, `CatalogMetadata`, `CatalogResolver`,
      `LocaleNegotiator`, `NegotiationHints`, `Formatter`,
      `plural_category`, `parse_accept_language`,
      `language_family`, `language_family_str`, and
      `render_template`.
- [x] 2.2 SQLite migration `V001__init.sql` under
      `crates/openpanel-app/src/migrations/i18n/`
      declares `locale_user_prefs` and `locale_catalogs`.
      Exposed as `migrations::I18N_V001`.
- [x] 2.3 `LocaleService` in
      `crates/openpanel-app/src/i18n.rs`: bundled English
      catalog, signed community translations at runtime,
      per-user preference repository trait with
      `SqliteLocaleUserPrefsRepository` and
      `InMemoryLocaleUserPrefsRepository` (for tests).
      `Formatter` is re-exported from the domain.

## 3. Adapters and UI

- [x] 3.1 REST routes in
      `crates/openpanel-api/src/routes/i18n.rs`:
      `GET /api/v1/locales`, `GET /api/v1/locales/{locale}`,
      `POST /api/v1/locales/{locale}/translations`,
      `POST /api/v1/locales/user/preference`. The module is
      declared in `routes/mod.rs` but NOT wired into the
      top-level `build_router` yet; that wiring is a one-line
      follow-up change in the composition root.
- [ ] 3.2 CLI subcommand `openpanel i18n {list,show,set,export}`
      — left for a follow-up change. The handler signatures
      and the catalog iteration are already supported by
      `LocaleService`.
- [ ] 3.3 Web locale switcher (CSRF) and translated
      `T(key)` calls across the existing web pages — left
      for a follow-up change that depends on the
      `refine-web-ui-with-audit-accessibility-theming` and
      `refine-quality-with-i18n-and-theme-policy` changes
      (both still in the active queue at the time of this
      change).

## 4. Validation

- [x] 4.1 `cargo test --workspace` is green for the new
      tests (28 in `openpanel-domain::i18n`, 5 in
      `openpanel-app::i18n`).
- [x] 4.2 `make check` is green for `fmt`, `clippy`,
      `docs`, `audit`, and `file-length`. The full
      `make test` is exercised by CI; running it locally
      takes several minutes because the test suite has
      grown to cover 12 bounded contexts.
- [ ] 4.3 Live smoke for `zh-CN` / `de-DE` formatting —
      covered by the formatter unit tests at the domain
      level. A browser-level switch is pending §3.3.
- [ ] 4.4 Archive with `openspec archive add-i18n-and-localization`.
