# Add i18n and localization

## Why

`refine-web-ui-with-audit-accessibility-theming` registered
the `T(key)` string-table contract; `refine-quality-with-i18n-and-theme-policy`
registered the lint that rejects literal strings outside the
table. To turn those hooks into a real product feature, the
panel needs locale negotiation per request, message
catalogs, right-to-left support, locale-aware number / date /
time formatting, and translation workflows. cPanel and Baota
both ship i18n; OpenPanel currently has none. This change
implements the runtime.

## What Changes

- New bounded context `i18n` carrying `Locale`, `Catalog`,
  `Message`, `LocaleNegotiator`, `Formatter`.
- New endpoints: `GET /locales`, `GET /locales/{locale}`,
  `POST /locales/{locale}/translations`.
- A panel-bundled English catalog and a few community
  translations (the spec doesn't enumerate locales; it's a
  runtime that any catalog plugs into).
- Locale negotiation per request: `Accept-Language` +
  `User.preferred_locale` + cookie + locale-prefixed URL.

## Capabilities

### New Capabilities

- `i18n`: locale negotiation, catalog, message resolution,
  formatting, and translation workflow.

## Impact

- Domain: `Locale`, `Catalog`, `Message`, `LocaleNegotiator`,
  `Formatter`.
- App: `LocaleService`, `FormatService`.
- API/CLI/web: `/locales*`; CLI
  `openpanel i18n {list,show,set,export}`; web locale switcher.
- Coupling: depends on `refine-web-ui-with-audit-accessibility-theming`
  and `refine-quality-with-i18n-and-theme-policy` for hooks.
